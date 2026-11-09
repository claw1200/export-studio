/**
 * Where the playhead is between engine ticks.
 *
 * The engine reports each deck about thirty times a second. Every animation frame in between is
 * the last report plus the time since it arrived, and small disagreements between that guess and
 * the next report are eased away rather than jumped.
 */

/** A loop's in and out points, in seconds. */
export type LoopRange = readonly [start: number, end: number]

/** A deck as the engine last reported it. */
export interface Anchor {
  frames: number
  /** When the report arrived, from `performance.now()`. */
  at: number
  sampleRate: number
  playing: boolean
  /** Bumped on every load and seek; a new one is a deliberate move, so the playhead snaps. */
  generation: number
  /** Playback speed against real time. */
  rate: number
  /** An active loop in seconds, which playback wraps inside. */
  loop: LoopRange | null
}

export const NO_ANCHOR: Anchor = {
  frames: 0,
  at: 0,
  sampleRate: 0,
  playing: false,
  generation: 0,
  rate: 1,
  loop: null
}

function positiveModulo(n: number, divisor: number): number {
  return ((n % divisor) + divisor) % divisor
}

function loopLength([start, end]: LoopRange): number {
  return end - start
}

function isInside(seconds: number, [start, end]: LoopRange): boolean {
  return seconds >= start && seconds < end
}

function intoLoop(seconds: number, loop: LoopRange): number {
  const [start] = loop
  return start + positiveModulo(seconds - start, loopLength(loop))
}

/**
 * `seconds` folded back into `loop` once it passes the out point, as the engine does.
 * A playhead that has not reached the loop is left alone.
 */
export function wrap(seconds: number, loop: LoopRange | null): number {
  if (!loop || !(loopLength(loop) > 0) || seconds < loop[1]) return seconds
  return intoLoop(seconds, loop)
}

/** The engine lets a playhead already past the out point play on, so only one before it wraps. */
function playOn(from: number, seconds: number, loop: LoopRange | null): number {
  const to = from + seconds
  return loop && from < loop[1] ? wrap(to, loop) : to
}

function shortestWayRound(from: number, to: number, loop: LoopRange): number {
  const length = loopLength(loop)
  const forwards = positiveModulo(to - from, length)
  return forwards > length / 2 ? forwards - length : forwards
}

/** Seconds into the track at `now`. Never runs backwards from the anchor, except round a loop. */
export function extrapolate(anchor: Anchor, now: number): number {
  if (anchor.sampleRate <= 0) return 0
  const reported = anchor.frames / anchor.sampleRate
  if (!anchor.playing) return reported
  const elapsedSeconds = (Math.max(0, now - anchor.at) / 1000) * anchor.rate
  return playOn(reported, elapsedSeconds, anchor.loop)
}

/** A correction larger than this is a seek or a load rather than drift, and is taken at once. */
export const SNAP_SECONDS = 0.25

/** How quickly a small correction is absorbed: about 150 ms to close a gap. */
const EASE_MS = 50

/**
 * The position to draw: `shown` advanced by playback, plus part of the remaining gap to `target`.
 * Easing only the correction, not the motion, keeps the playhead from lagging behind the audio.
 */
export function follow(
  shown: number,
  target: number,
  sinceMs: number,
  advanceSeconds: number,
  loop: LoopRange | null = null
): number {
  const predicted = playOn(shown, advanceSeconds, loop)
  const bothInLoop = loop !== null && isInside(predicted, loop) && isInside(target, loop)
  const gap = bothInLoop ? shortestWayRound(predicted, target, loop) : target - predicted
  if (Math.abs(gap) > SNAP_SECONDS || sinceMs <= 0) return target
  const eased = predicted + gap * (1 - Math.exp(-sinceMs / EASE_MS))
  return bothInLoop ? intoLoop(eased, loop) : eased
}

/** An anchor standing still at `seconds`: what a drag or a paused seek shows. */
export function pinned(anchor: Anchor, seconds: number, now: number): Anchor {
  return {
    ...anchor,
    frames: anchor.sampleRate > 0 ? seconds * anchor.sampleRate : anchor.frames,
    at: now,
    playing: false
  }
}
