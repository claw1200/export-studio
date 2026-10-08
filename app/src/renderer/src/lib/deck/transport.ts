/**
 * What the deck's buttons do, decided without touching the engine. Each function takes the deck's
 * state and returns the action to carry out, which keeps the CDJ semantics testable on their own.
 */
import { firstIndexAtOrAfter, nearestBeat } from '../beat'
import type { Beatgrid } from '../types'

/** How close to the cue point still counts as being on it, in seconds. */
export const CUE_TOLERANCE = 0.001

export interface TransportState {
  position: number
  playing: boolean
  scrubbing: boolean
  cuePoint: number
}

export type CueAction =
  /** Stop and return to the cue point: CUE while playing. */
  | { kind: 'return'; at: number }
  /** Move the cue point here (and the playhead with it): CUE while paused elsewhere. */
  | { kind: 'set'; at: number }
  /** Only move the cue point: CUE during a drag. */
  | { kind: 'mark'; at: number }
  /** Play for as long as the button is held: CUE while paused on the cue point. */
  | { kind: 'hold'; at: number }

/** CUE pressed. `here` is the playhead, already quantised and clamped to the file. */
export function pressCue(state: TransportState, here: number): CueAction {
  if (state.scrubbing) return { kind: 'mark', at: here }
  if (state.playing) return { kind: 'return', at: state.cuePoint }
  if (Math.abs(here - state.cuePoint) >= CUE_TOLERANCE) return { kind: 'set', at: here }
  return { kind: 'hold', at: state.cuePoint }
}

export interface HotCueTarget {
  start: number
  /** Seconds, for a saved loop. */
  end: number | null
}

export type HotCueAction =
  /** Playing: jump and keep playing (entering the loop, for a loop cue). */
  | { kind: 'jump'; at: number; loop: [number, number] | null }
  /** Paused: play from the cue while held, then return to it. It becomes the cue point. */
  | { kind: 'hold'; at: number; loop: [number, number] | null }

export function pressHotCue(state: TransportState, cue: HotCueTarget): HotCueAction {
  const loop: [number, number] | null = cue.end != null && cue.end > cue.start ? [cue.start, cue.end] : null
  return state.playing && !state.scrubbing
    ? { kind: 'jump', at: cue.start, loop }
    : { kind: 'hold', at: cue.start, loop }
}

/** A held CUE or hot cue, and whether pressing play during it made it stick. */
export interface Hold {
  at: number
  latched: boolean
}

/** Where releasing a held button returns to, or null when play latched it. */
export function releaseHold(hold: Hold | null): number | null {
  if (!hold || hold.latched) return null
  return hold.at
}

/** Seconds of a beat at `seconds`, from the grid's tempo there, or null without a tempo. */
function beatSeconds(seconds: number, grid: Beatgrid | null, fallbackBpm: number): number | null {
  if (grid?.times.length) {
    const i = Math.max(0, Math.min(grid.times.length - 1, firstIndexAtOrAfter(grid.times, seconds * 1000) - 1))
    const bpm = grid.bpms[i]
    if (bpm > 0) return 60 / bpm
  }
  return fallbackBpm > 0 ? 60 / fallbackBpm : null
}

/**
 * A loop of `beats` beats from `seconds`: the in point on the nearest beat when quantising, and the
 * out point that many grid beats later. Past the grid's end the local tempo carries on.
 */
export function beatLoop(
  seconds: number,
  beats: number,
  grid: Beatgrid | null,
  fallbackBpm: number,
  quantise: boolean
): [number, number] | null {
  if (!(beats > 0)) return null
  const start = quantise ? nearestBeat(seconds, grid) : seconds
  const times = grid?.times ?? []
  const at = firstIndexAtOrAfter(times, start * 1000)
  const onBeat = at < times.length && Math.abs(times[at] - start * 1000) < 0.5
  if (onBeat && Number.isInteger(beats) && at + beats < times.length) return [start, times[at + beats] / 1000]
  const beat = beatSeconds(start, grid, fallbackBpm)
  return beat ? [start, start + beats * beat] : null
}

/** A loop half or twice as long, from the same in point. */
export function resizeLoop(loop: [number, number], factor: number): [number, number] | null {
  const length = (loop[1] - loop[0]) * factor
  return length > 0.01 ? [loop[0], loop[0] + length] : null
}
