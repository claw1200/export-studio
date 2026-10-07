import type { Beatgrid } from './types'

export function firstIndexAtOrAfter(times: readonly number[], ms: number): number {
  let lo = 0
  let hi = times.length
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (times[mid] < ms) lo = mid + 1
    else hi = mid
  }
  return lo
}

function lastIndexAtOrBefore(times: readonly number[], ms: number): number {
  let lo = 0
  let hi = times.length - 1
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (times[mid] <= ms) lo = mid
    else hi = mid - 1
  }
  return lo
}

export function nearestBeat(seconds: number, grid: Beatgrid | null): number {
  const times = grid?.times
  if (!grid || !times?.length) return seconds
  const ms = seconds * 1000
  const last = times.length - 1
  if (ms <= times[0]) return beatFromAnchor(ms, times[0], grid.bpms[0]) / 1000
  if (ms >= times[last]) return beatFromAnchor(ms, times[last], grid.bpms[last]) / 1000

  const nextIndex = firstIndexAtOrAfter(times, ms)
  const previous = times[nextIndex - 1]
  const next = times[nextIndex]
  return (ms - previous <= next - ms ? previous : next) / 1000
}

function beatFromAnchor(ms: number, anchor: number, bpm: number): number {
  if (!(bpm > 0)) return Math.max(0, anchor)
  const step = 60_000 / bpm
  let steps = Math.round((ms - anchor) / step)
  let snapped = anchor + steps * step
  if (snapped < 0) {
    steps = Math.ceil(-anchor / step)
    snapped = anchor + steps * step
  }
  return snapped
}

const BEATS_PER_BAR = 4
const barCounts = new WeakMap<Beatgrid, Int32Array>()

function barsThrough(grid: Beatgrid): Int32Array {
  let counts = barCounts.get(grid)
  if (!counts) {
    counts = new Int32Array(grid.beats.length)
    let bars = 0
    for (let i = 0; i < grid.beats.length; i++) {
      if (grid.beats[i] === 1) bars++
      counts[i] = bars
    }
    barCounts.set(grid, counts)
  }
  return counts
}

function beatIndexAt(grid: Beatgrid, ms: number): number {
  return ms < grid.times[0] ? -1 : lastIndexAtOrBefore(grid.times, ms)
}

function nearestBeatIndex(times: readonly number[], ms: number): number {
  const next = firstIndexAtOrAfter(times, ms)
  if (next === 0) return 0
  if (next === times.length) return times.length - 1
  return ms - times[next - 1] <= times[next] - ms ? next - 1 : next
}

export function barBeatAt(seconds: number, grid: Beatgrid | null): string | null {
  if (!grid?.times.length) return null
  const index = beatIndexAt(grid, seconds * 1000)
  if (index < 0) return null
  return `${barsThrough(grid)[index]}.${grid.beats[index]}`
}

export function countdownTo(seconds: number, targetSeconds: number, grid: Beatgrid | null): string | null {
  if (!grid?.times.length) return null
  const beats = Math.max(0, nearestBeatIndex(grid.times, targetSeconds * 1000) - beatIndexAt(grid, seconds * 1000))
  return `${Math.floor(beats / BEATS_PER_BAR)}.${beats % BEATS_PER_BAR}`
}

export function bpmAt(seconds: number, grid: Beatgrid | null, fallback = 0): number {
  if (!grid?.times.length) return fallback
  return grid.bpms[lastIndexAtOrBefore(grid.times, seconds * 1000)]
}

export interface GridDraft extends Beatgrid {
  /** Where beat 0 sits, counted in beats of the saved grid. Lets cues follow the edit. */
  origin: number
  /** Saved-grid beats per draft beat. */
  scale: number
}

export const MIN_GRID_BPM = 40
export const MAX_GRID_BPM = 300
const FALLBACK_STEP_MS = 500

export function draftOf(grid: Beatgrid): GridDraft {
  return { times: [...grid.times], beats: [...grid.beats], bpms: [...grid.bpms], origin: 0, scale: 1 }
}

function stepAt(bpms: readonly number[], index: number): number {
  const bpm = bpms[index]
  return bpm > 0 ? 60_000 / bpm : FALLBACK_STEP_MS
}

function cycleBeat(beat: number, delta: number): number {
  return ((((beat - 1 + delta) % BEATS_PER_BAR) + BEATS_PER_BAR) % BEATS_PER_BAR) + 1
}

function fractionalIndex(grid: Beatgrid, ms: number): number {
  const { times, bpms } = grid
  const last = times.length - 1
  if (ms < times[0]) return (ms - times[0]) / stepAt(bpms, 0)
  if (ms >= times[last]) return last + (ms - times[last]) / stepAt(bpms, last)
  const i = lastIndexAtOrBefore(times, ms)
  const span = times[i + 1] - times[i]
  return i + (span > 0 ? (ms - times[i]) / span : 0)
}

function timeAtIndex(grid: Beatgrid, index: number): number {
  const { times, bpms } = grid
  const last = times.length - 1
  if (index <= 0) return times[0] + index * stepAt(bpms, 0)
  if (index >= last) return times[last] + (index - last) * stepAt(bpms, last)
  const i = Math.floor(index)
  return times[i] + (index - i) * (times[i + 1] - times[i])
}

function fitToTrack(draft: GridDraft, durationMs: number): GridDraft {
  const times = [...draft.times]
  const beats = [...draft.beats]
  const bpms = [...draft.bpms]
  let origin = draft.origin
  const { scale } = draft
  while (times.length > 1 && times[0] < 0) {
    times.shift()
    beats.shift()
    bpms.shift()
    origin += scale
  }
  while (times[0] - stepAt(bpms, 0) >= 0) {
    times.unshift(times[0] - stepAt(bpms, 0))
    beats.unshift(cycleBeat(beats[0], -1))
    bpms.unshift(bpms[0])
    origin -= scale
  }
  while (times.length > 1 && times[times.length - 1] > durationMs) {
    times.pop()
    beats.pop()
    bpms.pop()
  }
  for (let last = times.length - 1; times[last] + stepAt(bpms, last) <= durationMs; last++) {
    times.push(times[last] + stepAt(bpms, last))
    beats.push(cycleBeat(beats[last], 1))
    bpms.push(bpms[last])
  }
  return { times, beats, bpms, origin, scale }
}

export function shiftGrid(draft: GridDraft, ms: number, durationMs: number): GridDraft {
  const { times } = draft
  if (!times.length) return draft
  const offset = Math.max(-times[times.length - 1], Math.min(Math.max(0, durationMs - times[0]), ms))
  return fitToTrack({ ...draft, times: times.map((t) => t + offset) }, durationMs)
}

/** Moves the beat nearest `ms` onto `ms` and makes it a downbeat. */
export function setDownbeat(draft: GridDraft, ms: number, durationMs: number): GridDraft {
  if (!draft.times.length) return draft
  const shifted = shiftGrid(draft, ms - draft.times[nearestBeatIndex(draft.times, ms)], durationMs)
  const delta = 1 - shifted.beats[nearestBeatIndex(shifted.times, ms)]
  return delta ? { ...shifted, beats: shifted.beats.map((b) => cycleBeat(b, delta)) } : shifted
}

/** Constant-tempo grid through the beat nearest `anchorMs`. `density` is draft beats per old draft beat. */
export function retempoGrid(
  draft: GridDraft,
  bpm: number,
  anchorMs: number,
  durationMs: number,
  density = 1
): GridDraft {
  if (!draft.times.length) return draft
  const tempo = Math.round(Math.max(MIN_GRID_BPM, Math.min(MAX_GRID_BPM, bpm)) * 100) / 100
  const anchor = nearestBeatIndex(draft.times, anchorMs)
  const anchorMsExact = draft.times[anchor]
  const anchorCoord = draft.origin + anchor * draft.scale
  const scale = draft.scale / density
  const step = 60_000 / tempo
  const before = Math.floor(anchorMsExact / step)
  const end = Math.max(durationMs, anchorMsExact)
  const times: number[] = []
  const beats: number[] = []
  const bpms: number[] = []
  for (let j = 0; ; j++) {
    const t = anchorMsExact + (j - before) * step
    if (t > end + 0.5) break
    times.push(t)
    beats.push(cycleBeat(draft.beats[anchor], j - before))
    bpms.push(tempo)
  }
  return { times, beats, bpms, origin: anchorCoord - before * scale, scale }
}

/** Moves a time from one grid onto another, keeping its position relative to the beats. */
export function remapTime(ms: number, from: Beatgrid | GridDraft, to: GridDraft): number {
  if (!from.times.length || !to.times.length) return ms
  const coord = 'origin' in from ? from.origin + fractionalIndex(from, ms) * from.scale : fractionalIndex(from, ms)
  return timeAtIndex(to, (coord - to.origin) / to.scale)
}
