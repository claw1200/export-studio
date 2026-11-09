import { BEATS_PER_BAR, beatMs, firstIndexAtOrAfter, nearestBeatIndex } from './beat'
import type { Beatgrid } from './types'

export const MIN_GRID_BPM = 40
export const MAX_GRID_BPM = 300
export const MIN_RAMP_BEATS = BEATS_PER_BAR
export const VARIES_DESCRIPTION = 'The tempo here varies beat by beat, as rekordbox analysed it'
const MIN_STEADY_BEATS = BEATS_PER_BAR
const MIN_BEATS_BEFORE_NEXT_FLAG = 0.25
const MIN_BEATS_BETWEEN_FLAGS = 0.75
const STORED_BPM_STEPS_PER_BPM = 100
const BPM_ROUNDING_TOLERANCE = 0.5 / STORED_BPM_STEPS_PER_BPM + 0.0001
const RAMP_STEP_TOLERANCE = 2 / STORED_BPM_STEPS_PER_BPM + 0.0001
const BEAT_GAP_TOLERANCE_MS = 2
const MAX_BEATS = 1 << 20

interface LoadedBeats {
  times: number[]
  bpms: number[]
}

export interface TempoFlag {
  id: number
  startMs: number
  bpm: number
  rampBeats: number
  beatNumber: number | null
  loaded: LoadedBeats | null
  savedBeat: number
  savedBeatsPerBeat: number
}

export interface TempoMap {
  flags: TempoFlag[]
  durationMs: number
}

export interface CompiledGrid extends Beatgrid {
  sectionOfBeat: number[]
  stepInSection: number[]
  sectionStart: number[]
}

let nextId = 1
export const newFlagId = (): number => nextId++

export const roundBpm = (bpm: number): number => Math.round(bpm * STORED_BPM_STEPS_PER_BPM) / STORED_BPM_STEPS_PER_BPM
const clampBpm = (bpm: number): number => roundBpm(Math.max(MIN_GRID_BPM, Math.min(MAX_GRID_BPM, bpm)))
const cycleBeat = (beat: number, delta: number): number =>
  ((((beat - 1 + delta) % BEATS_PER_BAR) + BEATS_PER_BAR) % BEATS_PER_BAR) + 1
const last = <T>(values: readonly T[]): T => values[values.length - 1]
const mean = (values: number[]): number => values.reduce((a, b) => a + b, 0) / values.length
const isSteady = (bpms: readonly number[]): boolean => bpms.every((bpm) => bpm === bpms[0])

export const endBpm = (flag: TempoFlag): number => (flag.loaded?.bpms.length ? last(flag.loaded.bpms) : flag.bpm)

export const varies = (flag: TempoFlag): boolean => !!flag.loaded && !flag.rampBeats && !isSteady(flag.loaded.bpms)

export function rampTempo(rampBeats: number, fromBpm: number, toBpm: number, step: number): number {
  const ramping = step >= 0 && step < rampBeats
  return ramping ? fromBpm + ((toBpm - fromBpm) * (step + 1)) / rampBeats : toBpm
}

export const sectionEnd = (grid: CompiledGrid, j: number): number => grid.sectionStart[j + 1] ?? grid.times.length

export function sectionAt(map: TempoMap, ms: number): number {
  return Math.max(
    0,
    map.flags.findLastIndex((flag) => flag.startMs <= ms + 0.5)
  )
}

export function compileTempoMap(map: TempoMap): CompiledGrid {
  const grid: CompiledGrid = { times: [], beats: [], bpms: [], sectionOfBeat: [], stepInSection: [], sectionStart: [] }
  const push = (time: number, bpm: number, section: number, step: number): void => {
    grid.times.push(time)
    grid.bpms.push(bpm)
    grid.sectionOfBeat.push(section)
    grid.stepInSection.push(step)
  }
  const { flags, durationMs } = map
  const first = flags[0]
  const leadInBpm = first.loaded?.bpms[0] ?? first.bpm
  for (let step = -Math.floor(first.startMs / beatMs(leadInBpm)); step < 0; step++)
    push(first.startMs + step * beatMs(leadInBpm), leadInBpm, 0, step)

  flags.forEach((flag, j) => {
    grid.sectionStart[j] = grid.times.length
    const next = flags[j + 1]
    const fromBpm = j > 0 ? endBpm(flags[j - 1]) : flag.bpm
    let time = flag.startMs
    let bpm = flag.bpm
    for (let step = 0; step < MAX_BEATS; step++) {
      if (flag.loaded && step < flag.loaded.times.length) {
        time = flag.loaded.times[step]
        bpm = flag.loaded.bpms[step]
      } else if (!flag.loaded) bpm = rampTempo(flag.rampBeats, fromBpm, flag.bpm, step)
      const pastEnd = next ? time > next.startMs - MIN_BEATS_BEFORE_NEXT_FLAG * beatMs(bpm) : time > durationMs
      if (step > 0 && pastEnd) break
      push(time, bpm, j, step)
      time += beatMs(bpm)
    }
  })
  grid.beats = numberBeats(flags, grid)
  return grid
}

function numberBeats(flags: TempoFlag[], grid: CompiledGrid): number[] {
  const beats = new Array<number>(grid.times.length)
  const anchor = grid.sectionStart[0]
  beats[anchor] = flags[0].beatNumber ?? 1
  for (let i = anchor - 1; i >= 0; i--) beats[i] = cycleBeat(beats[i + 1], -1)
  for (let i = anchor + 1; i < beats.length; i++) {
    const flagged = grid.stepInSection[i] === 0 ? flags[grid.sectionOfBeat[i]].beatNumber : null
    beats[i] = flagged ?? cycleBeat(beats[i - 1], 1)
  }
  return beats
}

interface Run {
  start: number
  end: number
  rampBeats: number
  varies: boolean
}

export function tempoMapOf(grid: Beatgrid, durationMs: number): TempoMap | null {
  const { times, beats, bpms } = grid
  if (!times.length) return null
  const barRestarts = new Set<number>([0])
  const gaps = new Set<number>()
  for (let i = 1; i < times.length; i++) {
    if (beats[i] !== cycleBeat(beats[i - 1], 1)) barRestarts.add(i)
    if (Math.abs(times[i] - times[i - 1] - beatMs(bpms[i - 1])) > BEAT_GAP_TOLERANCE_MS) gaps.add(i)
  }
  const bounds = [...barRestarts, times.length]
  const runs = bounds.slice(1).flatMap((end, b) => runsBetween(bpms, bounds[b], end, gaps))
  const flags = mergeIrregularRuns(runs, barRestarts).map((run): TempoFlag => {
    const runBpms = bpms.slice(run.start, run.end)
    const bpm = run.rampBeats ? runBpms[run.rampBeats - 1] : isSteady(runBpms) ? runBpms[0] : mean(runBpms)
    return {
      id: newFlagId(),
      startMs: times[run.start],
      bpm: roundBpm(bpm),
      rampBeats: run.rampBeats,
      beatNumber: beats[run.start],
      loaded: { times: times.slice(run.start, run.end), bpms: runBpms },
      savedBeat: run.start,
      savedBeatsPerBeat: 1
    }
  })
  return { flags, durationMs }
}

function runsBetween(bpms: readonly number[], start: number, end: number, gaps: ReadonlySet<number>): Run[] {
  const runs: Run[] = []
  for (let i = start; i < end;) {
    const rampBeats = i > 0 ? rampLengthAt(bpms, i, end, gaps) : 0
    let j = i + Math.max(1, rampBeats)
    while (j < end && !gaps.has(j) && bpms[j] === bpms[j - 1]) j++
    runs.push({ start: i, end: j, rampBeats, varies: false })
    i = j
  }
  return runs
}

function rampLengthAt(bpms: readonly number[], i: number, end: number, gaps: ReadonlySet<number>): number {
  const fromBpm = bpms[i - 1]
  const firstStep = bpms[i] - fromBpm
  if (!firstStep) return 0
  let length = 1
  while (i + length < end && !gaps.has(i + length)) {
    const step = bpms[i + length] - bpms[i + length - 1]
    if (Math.sign(step) !== Math.sign(firstStep) || Math.abs(step - firstStep) > RAMP_STEP_TOLERANCE) break
    length++
  }
  const fitsRamp = (length: number): boolean => {
    const toBpm = bpms[i + length - 1]
    return bpms
      .slice(i, i + length)
      .every((bpm, step) => Math.abs(bpm - rampTempo(length, fromBpm, toBpm, step)) <= BPM_ROUNDING_TOLERANCE)
  }
  for (; length >= MIN_RAMP_BEATS; length--) if (fitsRamp(length)) return length
  return 0
}

function mergeIrregularRuns(runs: Run[], barRestarts: ReadonlySet<number>): Run[] {
  const isShort = (run: Run): boolean => !run.rampBeats && run.end - run.start < MIN_STEADY_BEATS
  const merged: Run[] = []
  for (const run of runs) {
    const previous = merged[merged.length - 1]
    const mergeable = previous && !barRestarts.has(run.start) && (isShort(previous) || previous.varies)
    if (mergeable && isShort(run)) {
      previous.end = run.end
      previous.varies = true
    } else merged.push({ ...run })
  }
  return merged
}

function editFlag(map: TempoMap, j: number, change: Partial<TempoFlag>, keepLoadedBeats = false): TempoMap {
  const flags = [...map.flags]
  flags[j] = { ...flags[j], ...change }
  if (!keepLoadedBeats) {
    flags[j].loaded = null
    if (flags[j + 1]?.rampBeats) flags[j + 1] = { ...flags[j + 1], loaded: null }
  }
  return { ...map, flags }
}

function startLimits(map: TempoMap, j: number): [number, number] {
  const { flags } = map
  const previous = flags[j - 1]
  const next = flags[j + 1]
  const lo = previous ? previous.startMs + MIN_BEATS_BETWEEN_FLAGS * beatMs(endBpm(previous)) : -Infinity
  const hi = next ? next.startMs - MIN_BEATS_BETWEEN_FLAGS * beatMs(flags[j].bpm) : map.durationMs
  return [lo, hi]
}

function keepFirstFlagInTrack(flag: TempoFlag): TempoFlag {
  if (flag.startMs >= 0) return flag
  const { loaded } = flag
  const loadedInTrack = loaded && last(loaded.times) >= 0
  const beats = loadedInTrack ? firstIndexAtOrAfter(loaded.times, 0) : Math.ceil(-flag.startMs / beatMs(flag.bpm))
  return {
    ...flag,
    startMs: loadedInTrack ? loaded.times[beats] : flag.startMs + beats * beatMs(flag.bpm),
    loaded: loadedInTrack ? { times: loaded.times.slice(beats), bpms: loaded.bpms.slice(beats) } : null,
    savedBeat: flag.savedBeat + beats * flag.savedBeatsPerBeat,
    beatNumber: cycleBeat(flag.beatNumber ?? 1, beats)
  }
}

function withFirstFlagInTrack(map: TempoMap): TempoMap {
  const first = keepFirstFlagInTrack(map.flags[0])
  return first === map.flags[0] ? map : { ...map, flags: [first, ...map.flags.slice(1)] }
}

export function shiftSection(map: TempoMap, j: number, ms: number): TempoMap {
  const flag = map.flags[j]
  const [lo, hi] = startLimits(map, j)
  const delta = Math.max(lo, Math.min(hi, flag.startMs + ms)) - flag.startMs
  if (!delta) return map
  const loaded = flag.loaded && { ...flag.loaded, times: flag.loaded.times.map((t) => t + delta) }
  return withFirstFlagInTrack(editFlag(map, j, { startMs: flag.startMs + delta, loaded }, true))
}

export function moveFlag(map: TempoMap, j: number, ms: number): TempoMap {
  return shiftSection(map, j, ms - map.flags[j].startMs)
}

export function setSectionBpm(map: TempoMap, j: number, bpm: number, holdMs: number, density = 1): TempoMap {
  const flag = map.flags[j]
  const tempo = clampBpm(bpm)
  if (tempo === flag.bpm && density === 1 && !varies(flag)) return map
  const change: Partial<TempoFlag> = {
    bpm: tempo,
    rampBeats: Math.round(flag.rampBeats * density),
    savedBeatsPerBeat: flag.savedBeatsPerBeat / density
  }
  if (j === 0) Object.assign(change, holdBeatInFirstSection(map, holdMs, tempo, density))
  const edited = withFirstFlagInTrack(editFlag(map, j, change))
  const [first, second] = edited.flags
  return second && first.startMs >= second.startMs ? map : edited
}

function holdBeatInFirstSection(map: TempoMap, ms: number, tempo: number, density: number): Partial<TempoFlag> {
  const grid = compileTempoMap(map)
  const at = nearestBeatIndex(grid.times, ms)
  if (grid.sectionOfBeat[at] !== 0) return {}
  const flag = map.flags[0]
  const step = grid.stepInSection[at]
  const newStep = Math.round(step * density)
  return {
    startMs: grid.times[at] - newStep * beatMs(tempo),
    beatNumber: cycleBeat(grid.beats[at], -newStep),
    savedBeat: flag.savedBeat + step * flag.savedBeatsPerBeat - (newStep * flag.savedBeatsPerBeat) / density
  }
}

export function stretchSection(map: TempoMap, j: number, step: number, targetMs: number): TempoMap {
  const flag = map.flags[j]
  const span = targetMs - flag.startMs
  if (!step || Math.sign(span) !== Math.sign(step) || Math.abs(span) < 1) return map
  const steady = j === 0 || !flag.rampBeats || step < 0
  const bpm = steady ? (60_000 * step) / span : rampTargetFor(flag, endBpm(map.flags[j - 1]), step, span)
  return editFlag(map, j, { bpm: clampBpm(bpm) })
}

function rampTargetFor(flag: TempoFlag, fromBpm: number, steps: number, span: number): number {
  const lengthAt = (toBpm: number): number => {
    let total = 0
    for (let step = 0; step < steps; step++) total += beatMs(rampTempo(flag.rampBeats, fromBpm, toBpm, step))
    return total
  }
  let lo = MIN_GRID_BPM
  let hi = MAX_GRID_BPM
  for (let i = 0; i < 50; i++) {
    const mid = (lo + hi) / 2
    if (lengthAt(mid) > span) lo = mid
    else hi = mid
  }
  return (lo + hi) / 2
}

export function setDownbeat(map: TempoMap, ms: number): TempoMap {
  const grid = compileTempoMap(map)
  const at = nearestBeatIndex(grid.times, ms)
  const j = grid.sectionOfBeat[at]
  const shifted = shiftSection(map, j, ms - grid.times[at])
  return editFlag(shifted, j, { beatNumber: cycleBeat(1, -grid.stepInSection[at]) }, true)
}

export function addFlag(map: TempoMap, ms: number): { map: TempoMap; id: number } | null {
  const grid = compileTempoMap(map)
  const at = nearestBeatIndex(grid.times, ms)
  const j = grid.sectionOfBeat[at]
  const step = grid.stepInSection[at]
  if (step <= 0) return null
  const flag = map.flags[j]
  const end = sectionEnd(grid, j)
  const bpms = grid.bpms.slice(at, end)
  const added: TempoFlag = {
    id: newFlagId(),
    startMs: grid.times[at],
    bpm: roundBpm(last(bpms)),
    rampBeats: 0,
    beatNumber: grid.beats[at],
    loaded: { times: grid.times.slice(at, end), bpms },
    savedBeat: flag.savedBeat + step * flag.savedBeatsPerBeat,
    savedBeatsPerBeat: flag.savedBeatsPerBeat
  }
  const before = {
    ...flag,
    loaded: flag.loaded && { times: flag.loaded.times.slice(0, step), bpms: flag.loaded.bpms.slice(0, step) }
  }
  const flags = [...map.flags]
  flags.splice(j, 1, before, added)
  return { map: { ...map, flags }, id: added.id }
}

export function removeFlag(map: TempoMap, j: number): TempoMap {
  if (j <= 0 || j >= map.flags.length) return map
  const flags = map.flags.filter((_, i) => i !== j)
  const next = flags[j]
  if (next?.rampBeats) flags[j] = { ...next, loaded: null }
  return { ...map, flags }
}

export function setFlagBpm(map: TempoMap, j: number, bpm: number): TempoMap {
  return editFlag(map, j, { bpm: clampBpm(bpm) })
}

export function setRamp(map: TempoMap, j: number, beats: number): TempoMap {
  const rampBeats = Math.max(0, Math.round(beats))
  if (j <= 0 || rampBeats === map.flags[j].rampBeats) return map
  return editFlag(map, j, { rampBeats })
}

export function setBarStart(map: TempoMap, j: number, on: boolean): TempoMap {
  return j > 0 ? editFlag(map, j, { beatNumber: on ? 1 : null }, true) : map
}

export function rampEndMs(grid: CompiledGrid, map: TempoMap, j: number): number | null {
  const rampBeats = map.flags[j]?.rampBeats
  if (!rampBeats || j === 0) return null
  const at = grid.sectionStart[j] + rampBeats
  return at < sectionEnd(grid, j) ? grid.times[at] : null
}

export function continuedBeats(grid: CompiledGrid, j: number, untilMs: number): number[] {
  const end = sectionEnd(grid, j)
  const times = grid.times.slice(grid.sectionStart[j], end)
  const step = beatMs(grid.bpms[end - 1])
  for (let time = last(times) + step; time < untilMs + step; time += step) times.push(time)
  return times
}

export function timeOfSavedBeat(map: TempoMap, grid: CompiledGrid, savedBeat: number): number {
  const { flags } = map
  const j = Math.max(
    0,
    flags.findLastIndex((flag) => flag.savedBeat <= savedBeat)
  )
  const step = (savedBeat - flags[j].savedBeat) / flags[j].savedBeatsPerBeat
  const first = j === 0 ? 0 : grid.sectionStart[j]
  const lastBeat = sectionEnd(grid, j) - 1
  const extrapolate = (from: number): number =>
    grid.times[from] + (step - grid.stepInSection[from]) * beatMs(grid.bpms[from])
  const at = grid.sectionStart[j] + Math.floor(step)
  if (at < first) return extrapolate(first)
  if (at >= lastBeat) return extrapolate(lastBeat)
  return grid.times[at] + (step - Math.floor(step)) * (grid.times[at + 1] - grid.times[at])
}
