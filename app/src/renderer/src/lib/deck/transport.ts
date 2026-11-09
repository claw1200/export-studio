// Pure, so the CDJ semantics can be tested without the engine.
import { firstIndexAtOrAfter, nearestBeat } from '../beat'
import type { Beatgrid, Cue } from '../types'
import type { LoopRange, Seconds } from './clock'
import type { DeckLoop } from './deck.svelte'

export const CUE_TOLERANCE: Seconds = 0.001
export const SHORTEST_LOOP: Seconds = 0.01
const ON_BEAT_TOLERANCE_MS = 0.5

export interface TransportState {
  position: Seconds
  playing: boolean
  scrubbing: boolean
  cuePoint: Seconds
}

export type CueAction =
  | { kind: 'stopAtCue'; at: Seconds }
  | { kind: 'setCueAndGo'; at: Seconds }
  | { kind: 'setCueOnly'; at: Seconds }
  | { kind: 'holdFromCue'; at: Seconds }

export function pressCue(state: TransportState, quantisedPlayhead: Seconds): CueAction {
  if (state.scrubbing) return { kind: 'setCueOnly', at: quantisedPlayhead }
  if (state.playing) return { kind: 'stopAtCue', at: state.cuePoint }
  const awayFromCue = Math.abs(quantisedPlayhead - state.cuePoint) >= CUE_TOLERANCE
  if (awayFromCue) return { kind: 'setCueAndGo', at: quantisedPlayhead }
  return { kind: 'holdFromCue', at: state.cuePoint }
}

export interface CueRange {
  start: Seconds
  end: Seconds | null
}

export function cueRange(cue: Cue): CueRange {
  return { start: cue.start / 1000, end: cue.end == null ? null : cue.end / 1000 }
}

export function loopOf({ start, end }: CueRange): LoopRange | null {
  return end != null && end > start ? [start, end] : null
}

export type HotCueAction =
  { kind: 'jump'; at: Seconds; loop: LoopRange | null } | { kind: 'hold'; at: Seconds; loop: LoopRange | null }

export function pressHotCue(state: TransportState, cue: CueRange): HotCueAction {
  const loop = loopOf(cue)
  return state.playing && !state.scrubbing
    ? { kind: 'jump', at: cue.start, loop }
    : { kind: 'hold', at: cue.start, loop }
}

/** As on rekordbox. */
export function cueToStore(at: Seconds, loop: DeckLoop | null): CueRange {
  const cueOnly = { start: at, end: null }
  if (!loop || !(loop.end > loop.start)) return cueOnly
  const atLoopIn = Math.abs(at - loop.start) < CUE_TOLERANCE
  const savesLoop = loop.active || atLoopIn
  return savesLoop ? { start: loop.start, end: loop.end } : cueOnly
}

export interface Hold {
  at: Seconds
  latched: boolean
}

export function releaseHold(hold: Hold | null): Seconds | null {
  if (!hold || hold.latched) return null
  return hold.at
}

function secondsPerBeat(bpm: number): Seconds {
  return 60 / bpm
}

function beatLength(at: Seconds, grid: Beatgrid | null, fallbackBpm: number): Seconds | null {
  if (grid?.times.length) {
    const beatAtOrBefore = firstIndexAtOrAfter(grid.times, at * 1000) - 1
    const bpm = grid.bpms[Math.max(0, Math.min(grid.times.length - 1, beatAtOrBefore))]
    if (bpm > 0) return secondsPerBeat(bpm)
  }
  return fallbackBpm > 0 ? secondsPerBeat(fallbackBpm) : null
}

export function beatLoop(
  from: Seconds,
  beats: number,
  grid: Beatgrid | null,
  fallbackBpm: number,
  quantise: boolean
): LoopRange | null {
  if (!(beats > 0)) return null
  const start = quantise ? nearestBeat(from, grid) : from
  const times = grid?.times ?? []
  const at = firstIndexAtOrAfter(times, start * 1000)
  const onBeat = at < times.length && Math.abs(times[at] - start * 1000) < ON_BEAT_TOLERANCE_MS
  const gridReachesEnd = Number.isInteger(beats) && at + beats < times.length
  if (onBeat && gridReachesEnd) return [start, times[at + beats] / 1000]
  const beat = beatLength(start, grid, fallbackBpm)
  return beat ? [start, start + beats * beat] : null
}

export function resizeLoop([start, end]: LoopRange, factor: number): LoopRange | null {
  const length = (end - start) * factor
  return length > SHORTEST_LOOP ? [start, start + length] : null
}
