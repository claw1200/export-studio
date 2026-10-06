import type { Beatgrid } from './types'

/**
 * Nearest beat to `seconds`.
 * Inside the stored grid this is the closest marker. Outside it, the local tempo
 * is extended so the intro and outro still land on a beat.
 */
export function nearestBeat(seconds: number, grid: Beatgrid | null): number {
  const times = grid?.times
  if (!grid || !times?.length) return seconds
  const ms = seconds * 1000
  const last = times.length - 1
  if (ms <= times[0]) return extend(ms, times[0], grid.bpms[0]) / 1000
  if (ms >= times[last]) return extend(ms, times[last], grid.bpms[last]) / 1000

  let lo = 1
  let hi = last
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (times[mid] < ms) lo = mid + 1
    else hi = mid
  }
  const prev = times[lo - 1]
  const next = times[lo]
  return (ms - prev <= next - ms ? prev : next) / 1000
}

/** Beat at `anchor` ms, stepped by `bpm` to the closest point that is not before 0. */
function extend(ms: number, anchor: number, bpm: number): number {
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
