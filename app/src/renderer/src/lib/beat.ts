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
