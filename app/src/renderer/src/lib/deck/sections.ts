import { BEATS_PER_BAR, fractionalBeatIndex } from '../beat'
import type { Beatgrid, Cue } from '../types'

/** Cues closer together than this mark the same place, e.g. a hot cue on a memory cue. */
const SAME_PLACE_SECONDS = 0.05

/** The stretch of a track from one cue to the next, in seconds, and how many bars it lasts. */
export interface CueSection {
  start: number
  end: number
  /** Null without a beatgrid or a tempo to count by. */
  bars: number | null
  /**
   * `in` runs from the start of the track to the first cue and `out` from the last cue to the end:
   * the intro and outro, labelled rather than counted.
   */
  edge?: 'in' | 'out'
}

/** Bars from `start` to `end` seconds: along the grid when there is one, else at `bpm`. */
export function barsBetween(start: number, end: number, grid: Beatgrid | null, bpm: number): number | null {
  if (!(end > start)) return 0
  if (grid?.times.length) {
    const beats = fractionalBeatIndex(grid, end * 1000) - fractionalBeatIndex(grid, start * 1000)
    return beats / BEATS_PER_BAR
  }
  return bpm > 0 ? ((end - start) * bpm) / 60 / BEATS_PER_BAR : null
}

/**
 * The sections between consecutive cue points, hot and memory alike, in time order. A first cue
 * after the start of the track opens with an in section; with `trackEnd`, the stretch from the
 * last cue to the end follows as the out section.
 */
export function cueSections(cues: readonly Cue[], grid: Beatgrid | null, bpm: number, trackEnd?: number): CueSection[] {
  const starts: number[] = []
  for (const seconds of cues.map((cue) => cue.start / 1000).sort((a, b) => a - b)) {
    if (!starts.length || seconds - starts[starts.length - 1] >= SAME_PLACE_SECONDS) starts.push(seconds)
  }
  const sections: CueSection[] = starts
    .slice(1)
    .map((end, i) => ({ start: starts[i], end, bars: barsBetween(starts[i], end, grid, bpm) }))
  const first = starts[0]
  if (first != null && first >= SAME_PLACE_SECONDS) {
    sections.unshift({ start: 0, end: first, bars: barsBetween(0, first, grid, bpm), edge: 'in' })
  }
  const last = starts[starts.length - 1]
  if (last != null && trackEnd != null && trackEnd - last >= SAME_PLACE_SECONDS) {
    sections.push({ start: last, end: trackEnd, bars: barsBetween(last, trackEnd, grid, bpm), edge: 'out' })
  }
  return sections
}

/** A bar count as a label: whole bars plainly, anything else to the nearest quarter. */
export function formatBars(bars: number): string {
  const whole = Math.round(bars)
  if (Math.abs(bars - whole) < 0.05) return String(whole)
  const quarters = Math.round(bars * 4) / 4
  return String(quarters)
}
