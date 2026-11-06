/**
 * Dynamic beatgrids as tempo flags.
 *
 * Rekordbox stores a grid as one record per beat (time, beat number, tempo). While editing, the grid is
 * held as a list of flags instead: each flag starts a section that either jumps to its tempo or ramps
 * there from the previous section over a number of beats. Flags are never stored; they are read back
 * from the per-beat grid (`tempoMapOf`) and turned into per-beat records again on save (`compileTempoMap`).
 *
 * A section keeps the beats it was loaded with (`source`) until it is edited, so opening and saving a grid
 * changes nothing, and grids whose tempo wobbles beat by beat (rekordbox's dynamic analysis) survive intact.
 */
import type { Beatgrid } from './types'

export const MIN_GRID_BPM = 40
export const MAX_GRID_BPM = 300
const BEATS_PER_BAR = 4
/** A generated beat closer than this many beats to the next flag is dropped, so no interval is tiny. */
const FLAG_GUARD = 0.25
/** Rounded tempos (0.01 BPM) can be off by this much from the ramp they came from. */
const RAMP_TOLERANCE = 0.0051
/** Tempo runs shorter than this, outside a ramp, are read as one section whose tempo varies. */
const MIN_STEADY_RUN = BEATS_PER_BAR
/** Two beats further apart than a beat at the stored tempo by more than this start a new section. */
const GAP_TOLERANCE_MS = 2
/** The shortest ramp, in beats: one bar. Shorter tempo steps read as a tempo that varies. */
export const MIN_RAMP = BEATS_PER_BAR

export interface TempoFlag {
  id: number
  /** The section's first beat, in ms. */
  ms: number
  /** The section's tempo: where a ramp ends up. For a section whose tempo varies, its average. */
  bpm: number
  /** Beats the ramp from the previous section's tempo takes; 0 jumps straight to `bpm`. */
  ramp: number
  /** Number of the flag's beat, 1 to 4, or null to carry on counting. Always set on the first flag. */
  beat: number | null
  /** The section's beats as loaded, kept until the section is edited. Starts at `ms`. */
  source: { times: number[]; bpms: number[] } | null
  /** Where the flag's beat sits in the saved grid, counted in its beats, and saved beats per section beat. */
  coord: number
  scale: number
}

export interface TempoMap {
  flags: TempoFlag[]
  durationMs: number
}

export interface CompiledGrid extends Beatgrid {
  /** The section each beat belongs to. */
  sections: number[]
  /** Each beat's position in its section: 0 on the flag, negative before the first flag. */
  steps: number[]
  /** Index of each flag's beat. */
  starts: number[]
}

let nextId = 1
export const newFlagId = (): number => nextId++

export const roundBpm = (bpm: number): number => Math.round(bpm * 100) / 100
const cycleBeat = (beat: number, delta: number): number =>
  ((((beat - 1 + delta) % BEATS_PER_BAR) + BEATS_PER_BAR) % BEATS_PER_BAR) + 1

function firstAtOrAfter(times: readonly number[], ms: number): number {
  let lo = 0
  let hi = times.length
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (times[mid] < ms) lo = mid + 1
    else hi = mid
  }
  return lo
}

export function nearestIndex(times: readonly number[], ms: number): number {
  const next = firstAtOrAfter(times, ms)
  if (next === 0) return 0
  if (next >= times.length) return times.length - 1
  return ms - times[next - 1] <= times[next] - ms ? next - 1 : next
}

/** The tempo a section ends on, which a following ramp starts from. */
export function endBpm(flag: TempoFlag): number {
  const bpms = flag.source?.bpms
  return bpms?.length ? bpms[bpms.length - 1] : flag.bpm
}

/** True when the section's loaded beats change tempo irregularly and it has not been edited since. */
export function varies(flag: TempoFlag): boolean {
  const bpms = flag.source?.bpms
  return !!bpms && flag.ramp === 0 && bpms.some((b) => b !== bpms[0])
}

/** Tempo of the interval after beat `k` of a generated section that starts from `from`. */
export function tempoAt(flag: TempoFlag, from: number, k: number, bpm = flag.bpm): number {
  return flag.ramp > 0 && k >= 0 && k < flag.ramp ? from + ((bpm - from) * (k + 1)) / flag.ramp : bpm
}

/** The per-beat grid the flags describe, from 0 to the end of the track. */
export function compileTempoMap(map: TempoMap): CompiledGrid {
  const { flags, durationMs } = map
  const times: number[] = []
  const bpms: number[] = []
  const sections: number[] = []
  const steps: number[] = []
  const starts: number[] = []
  const push = (t: number, bpm: number, section: number, step: number): void => {
    times.push(t)
    bpms.push(bpm)
    sections.push(section)
    steps.push(step)
  }
  for (let j = 0; j < flags.length; j++) {
    const flag = flags[j]
    const next = flags[j + 1]
    const from = j > 0 ? endBpm(flags[j - 1]) : flag.bpm
    if (j === 0) {
      const first = flag.source?.bpms[0] ?? flag.bpm
      const step = 60_000 / first
      for (let k = -Math.floor(flag.ms / step); k < 0; k++) push(flag.ms + k * step, first, 0, k)
    }
    starts[j] = times.length
    const source = flag.source
    let t = flag.ms
    let bpm = flag.bpm
    for (let k = 0; k < 1_000_000; k++) {
      const loaded = source && k < source.times.length
      if (loaded) {
        t = source.times[k]
        bpm = source.bpms[k]
      } else if (!source) bpm = j > 0 ? tempoAt(flag, from, k) : flag.bpm
      const interval = 60_000 / bpm
      if (k > 0 && (next ? t > next.ms - FLAG_GUARD * interval : t > durationMs)) break
      push(t, bpm, j, k)
      t += interval
    }
  }
  const beats = new Array<number>(times.length)
  const anchor = starts[0]
  beats[anchor] = flags[0].beat ?? 1
  for (let i = anchor - 1; i >= 0; i--) beats[i] = cycleBeat(beats[i + 1], -1)
  for (let i = anchor + 1; i < times.length; i++) {
    const set = steps[i] === 0 ? flags[sections[i]].beat : null
    beats[i] = set ?? cycleBeat(beats[i - 1], 1)
  }
  return { times, beats, bpms, sections, steps, starts }
}

interface Run {
  start: number
  end: number
  ramp: number
  varies?: boolean
}

/** Splits beats `start..end` into steady runs and ramps. A gap always ends a run. */
function runsOf(bpms: readonly number[], start: number, end: number, gaps: ReadonlySet<number>): Run[] {
  const runs: Run[] = []
  let i = start
  while (i < end) {
    const ramp = i > 0 ? rampFrom(bpms, i, end, gaps) : 0
    let j = i + Math.max(1, ramp)
    // A steady run, or the tempo a ramp arrives at and holds.
    while (j < end && !gaps.has(j) && bpms[j] === bpms[j - 1]) j++
    runs.push({ start: i, end: j, ramp })
    i = j
  }
  return runs
}

/** Length of the ramp that starts at beat `i` from the tempo before it, or 0. */
function rampFrom(bpms: readonly number[], i: number, end: number, gaps: ReadonlySet<number>): number {
  const from = bpms[i - 1]
  const first = bpms[i] - from
  if (!first) return 0
  let length = 1
  while (i + length < end && !gaps.has(i + length)) {
    const step = bpms[i + length] - bpms[i + length - 1]
    if (Math.sign(step) !== Math.sign(first) || Math.abs(step - first) > 0.0201) break
    length++
  }
  for (; length >= MIN_RAMP; length--) {
    const to = bpms[i + length - 1]
    let fits = true
    for (let k = 0; k < length && fits; k++)
      fits = Math.abs(bpms[i + k] - (from + ((to - from) * (k + 1)) / length)) <= RAMP_TOLERANCE
    if (fits) return length
  }
  return 0
}

/** Reads flags back from a per-beat grid. Each section keeps its beats as loaded. */
export function tempoMapOf(grid: Beatgrid, durationMs: number): TempoMap | null {
  const { times, beats, bpms } = grid
  const n = times.length
  if (!n) return null
  // The bar count restarting always starts a section. A beat that does not follow the one before at its
  // tempo (a cut) ends a run, but runs too short to be a steady tempo still merge across it.
  const restarts = new Set<number>([0])
  const gaps = new Set<number>()
  for (let i = 1; i < n; i++) {
    if (beats[i] !== cycleBeat(beats[i - 1], 1)) restarts.add(i)
    if (Math.abs(times[i] - times[i - 1] - 60_000 / bpms[i - 1]) > GAP_TOLERANCE_MS) gaps.add(i)
  }
  const bounds = [...restarts, n]

  const short = (r: Run): boolean => !r.ramp && r.end - r.start < MIN_STEADY_RUN
  const sections: Run[] = []
  for (let b = 0; b + 1 < bounds.length; b++) {
    for (const run of runsOf(bpms, bounds[b], bounds[b + 1], gaps)) {
      const last = sections[sections.length - 1]
      // Runs too short to be a real tempo merge into one section whose tempo varies.
      if (last && !restarts.has(run.start) && short(run) && (short(last) || last.varies)) {
        last.end = run.end
        last.varies = true
      } else sections.push({ ...run, varies: false })
    }
  }

  const flags: TempoFlag[] = sections.map((s) => {
    const slice = bpms.slice(s.start, s.end)
    const steady = s.ramp ? slice[s.ramp - 1] : slice.every((b) => b === slice[0]) ? slice[0] : mean(slice)
    return {
      id: newFlagId(),
      ms: times[s.start],
      bpm: roundBpm(steady),
      ramp: s.ramp,
      beat: beats[s.start],
      source: { times: times.slice(s.start, s.end), bpms: slice },
      coord: s.start,
      scale: 1
    }
  })
  return { flags, durationMs }
}

const mean = (values: number[]): number => values.reduce((a, b) => a + b, 0) / values.length

/* ---------- Edits. Each returns a new map; flags other than the one edited keep their place. ---------- */

const clampBpm = (bpm: number): number => roundBpm(Math.max(MIN_GRID_BPM, Math.min(MAX_GRID_BPM, bpm)))

function withFlags(map: TempoMap, flags: TempoFlag[]): TempoMap {
  return { ...map, flags }
}

/** The section a time falls in. Before the first flag counts as the first section. */
export function sectionAt(map: TempoMap, ms: number): number {
  let section = 0
  for (let j = 1; j < map.flags.length; j++) if (map.flags[j].ms <= ms + 0.5) section = j
  return section
}

/** Forgets the loaded beats of a section, and of a ramp after it that starts from its tempo. */
function regenerate(flags: TempoFlag[], j: number): void {
  flags[j] = { ...flags[j], source: null }
  const next = flags[j + 1]
  if (next?.ramp) flags[j + 1] = { ...next, source: null }
}

function limitsOf(flags: TempoFlag[], j: number, durationMs: number): [number, number] {
  const lo = j > 0 ? flags[j - 1].ms + (0.75 * 60_000) / endBpm(flags[j - 1]) : -Infinity
  const hi = flags[j + 1] ? flags[j + 1].ms - (0.75 * 60_000) / flags[j].bpm : durationMs
  return [lo, hi]
}

/** Keeps the first flag inside the track by moving it on whole beats, so its beats stay put. */
function normaliseStart(flag: TempoFlag): TempoFlag {
  if (flag.ms >= 0) return flag
  const source = flag.source
  let beats: number
  let ms: number
  if (source && source.times[source.times.length - 1] >= 0) {
    beats = firstAtOrAfter(source.times, 0)
    ms = source.times[beats]
  } else {
    beats = Math.ceil(-flag.ms / (60_000 / flag.bpm))
    ms = flag.ms + (beats * 60_000) / flag.bpm
  }
  return {
    ...flag,
    ms,
    source:
      source && ms === source.times[beats]
        ? { times: source.times.slice(beats), bpms: source.bpms.slice(beats) }
        : null,
    coord: flag.coord + beats * flag.scale,
    beat: cycleBeat(flag.beat ?? 1, beats)
  }
}

/** Moves section `j` (its flag and every beat in it) by `ms`. */
export function shiftSection(map: TempoMap, j: number, ms: number): TempoMap {
  const flags = [...map.flags]
  const flag = flags[j]
  const [lo, hi] = limitsOf(flags, j, map.durationMs)
  const delta = Math.max(lo, Math.min(hi, flag.ms + ms)) - flag.ms
  if (!delta) return map
  const source = flag.source && { ...flag.source, times: flag.source.times.map((t) => t + delta) }
  flags[j] = { ...flag, ms: flag.ms + delta, source }
  if (j === 0) flags[0] = normaliseStart(flags[0])
  return withFlags(map, flags)
}

/** Moves flag `j` to `ms`, taking its section with it. The flags around it stay. */
export function moveFlag(map: TempoMap, j: number, ms: number): TempoMap {
  return shiftSection(map, j, ms - map.flags[j].ms)
}

/**
 * Sets section `j`'s tempo (`density` × as many beats). Later sections keep their place. The first
 * section holds the beat nearest `holdMs`, as the single-tempo editor always has; the rest hold their flag.
 */
export function setSectionBpm(map: TempoMap, j: number, bpm: number, holdMs: number, density = 1): TempoMap {
  const tempo = clampBpm(bpm)
  const flags = [...map.flags]
  const flag = flags[j]
  if (tempo === flag.bpm && density === 1 && !varies(flag)) return map
  const ramp = Math.max(0, Math.round(flag.ramp * density))
  let updated: TempoFlag = { ...flag, bpm: tempo, ramp, scale: flag.scale / density }
  if (j === 0) {
    const grid = compileTempoMap(map)
    const at = nearestIndex(grid.times, holdMs)
    if (grid.sections[at] === 0) {
      // The held beat keeps its time, bar position and place in the saved grid.
      const k = Math.round(grid.steps[at] * density)
      const coordAt = flag.coord + grid.steps[at] * flag.scale
      updated = {
        ...updated,
        ms: grid.times[at] - (k * 60_000) / tempo,
        beat: cycleBeat(grid.beats[at], -k),
        coord: coordAt - k * updated.scale
      }
    }
  }
  flags[j] = updated
  regenerate(flags, j)
  if (j === 0) flags[0] = normaliseStart(flags[0])
  if (flags[1] && flags[0].ms >= flags[1].ms) return map
  return withFlags(map, flags)
}

/** Sets section `j`'s tempo so beat `k` of it lands on `targetMs`. */
export function stretchSection(map: TempoMap, j: number, k: number, targetMs: number): TempoMap {
  const flag = map.flags[j]
  const span = targetMs - flag.ms
  if (!k || Math.sign(span) !== Math.sign(k) || Math.abs(span) < 1) return map
  let bpm: number
  if (j === 0 || !flag.ramp || k < 0) bpm = (60_000 * k) / span
  else {
    const from = endBpm(map.flags[j - 1])
    const length = (b: number): number => {
      let total = 0
      for (let i = 0; i < k; i++) total += 60_000 / tempoAt(flag, from, i, b)
      return total
    }
    let lo = MIN_GRID_BPM
    let hi = MAX_GRID_BPM
    for (let i = 0; i < 50; i++) {
      const mid = (lo + hi) / 2
      if (length(mid) > span) lo = mid
      else hi = mid
    }
    bpm = (lo + hi) / 2
  }
  const flags = [...map.flags]
  flags[j] = { ...flag, bpm: clampBpm(bpm) }
  regenerate(flags, j)
  return withFlags(map, flags)
}

/** Moves the beat nearest `ms` onto `ms` and makes it a downbeat, within its section. */
export function setDownbeat(map: TempoMap, ms: number): TempoMap {
  const grid = compileTempoMap(map)
  const at = nearestIndex(grid.times, ms)
  const j = grid.sections[at]
  const shifted = shiftSection(map, j, ms - grid.times[at])
  const flags = [...shifted.flags]
  flags[j] = { ...flags[j], beat: cycleBeat(1, -grid.steps[at]) }
  return withFlags(shifted, flags)
}

/**
 * Adds a flag on the beat nearest `ms`. The grid does not change until the new flag is edited.
 * Returns null when that beat already has a flag or comes before the first one.
 */
export function addFlag(map: TempoMap, ms: number): { map: TempoMap; id: number } | null {
  const grid = compileTempoMap(map)
  const at = nearestIndex(grid.times, ms)
  const j = grid.sections[at]
  const k = grid.steps[at]
  if (k <= 0) return null
  const end = j + 1 < map.flags.length ? grid.starts[j + 1] : grid.times.length
  const flag = map.flags[j]
  const steady = grid.bpms.slice(at, end)
  const added: TempoFlag = {
    id: newFlagId(),
    ms: grid.times[at],
    bpm: roundBpm(steady[steady.length - 1] ?? flag.bpm),
    ramp: 0,
    beat: grid.beats[at],
    source: { times: grid.times.slice(at, end), bpms: steady },
    coord: flag.coord + k * flag.scale,
    scale: flag.scale
  }
  const before: TempoFlag = {
    ...flag,
    source: flag.source && { times: flag.source.times.slice(0, k), bpms: flag.source.bpms.slice(0, k) }
  }
  const flags = [...map.flags]
  flags.splice(j, 1, before, added)
  return { map: withFlags(map, flags), id: added.id }
}

/** Removes flag `j`; the section before it carries on at its tempo. */
export function removeFlag(map: TempoMap, j: number): TempoMap {
  if (j <= 0 || j >= map.flags.length) return map
  const flags = [...map.flags]
  flags.splice(j, 1)
  if (flags[j]?.ramp) flags[j] = { ...flags[j], source: null }
  return withFlags(map, flags)
}

export function setFlagBpm(map: TempoMap, j: number, bpm: number): TempoMap {
  const flags = [...map.flags]
  flags[j] = { ...flags[j], bpm: clampBpm(bpm) }
  regenerate(flags, j)
  return withFlags(map, flags)
}

/** Makes flag `j` ramp over `beats` beats, or jump when `beats` is 0. */
export function setRamp(map: TempoMap, j: number, beats: number): TempoMap {
  if (j <= 0) return map
  const ramp = Math.max(0, Math.round(beats))
  if (ramp === map.flags[j].ramp) return map
  const flags = [...map.flags]
  flags[j] = { ...flags[j], ramp }
  regenerate(flags, j)
  return withFlags(map, flags)
}

export function setBarStart(map: TempoMap, j: number, on: boolean): TempoMap {
  if (j <= 0) return map
  const flags = [...map.flags]
  flags[j] = { ...flags[j], beat: on ? 1 : null }
  return withFlags(map, flags)
}

/** Time of the beat ending flag `j`'s ramp, or null when it jumps or the ramp runs past the section. */
export function rampEndMs(grid: CompiledGrid, map: TempoMap, j: number): number | null {
  const ramp = map.flags[j]?.ramp
  if (!ramp || j === 0) return null
  const at = grid.starts[j] + ramp
  return at < grid.times.length && grid.sections[at] === j ? grid.times[at] : null
}

/** Beat times of section `j` carried on past its end, for snapping the next flag onto them. */
export function continuedBeats(grid: CompiledGrid, map: TempoMap, j: number, untilMs: number): number[] {
  const end = j + 1 < map.flags.length ? grid.starts[j + 1] : grid.times.length
  const out = grid.times.slice(grid.starts[j], end)
  let t = out[out.length - 1]
  const step = 60_000 / grid.bpms[end - 1]
  while (t < untilMs) out.push((t += step))
  return out
}

/** Where a saved-grid coordinate (fractional beat index) lands in the edited grid, in ms. */
export function timeAtCoord(map: TempoMap, grid: CompiledGrid, coord: number): number {
  const { flags } = map
  let j = 0
  for (let i = 1; i < flags.length; i++) if (flags[i].coord <= coord) j = i
  const k = (coord - flags[j].coord) / flags[j].scale
  const start = j === 0 ? 0 : grid.starts[j]
  const end = j + 1 < flags.length ? grid.starts[j + 1] : grid.times.length
  const at = grid.starts[j] + Math.floor(k)
  if (at < start) return grid.times[start] + (k - grid.steps[start]) * (60_000 / grid.bpms[start])
  if (at >= end - 1) return grid.times[end - 1] + (k - grid.steps[end - 1]) * (60_000 / grid.bpms[end - 1])
  return grid.times[at] + (k - Math.floor(k)) * (grid.times[at + 1] - grid.times[at])
}
