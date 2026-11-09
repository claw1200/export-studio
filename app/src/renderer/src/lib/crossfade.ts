export const FADER_CENTRE = 0.5
/** How close to the middle snaps to it, so the centre is easy to find by hand. */
export const CENTRE_SNAP = 0.02

/** The sharpest curve: a deck still fades out over the last stretch of travel rather than clicking off. */
export const CURVE_MAX = 0.95
/** The curve the fader had before it could be set: full level to the centre, then a quarter cosine. */
export const DEFAULT_CURVE = 0.5

/**
 * Levels for a crossfader at `position`, 0 all A to 1 all B. `curve` is how far a deck holds full
 * level (0 dB) as the fader moves away from it, as a fraction of the travel: 0 fades it from its
 * own end, so both decks are at −3 dB in the centre with constant power throughout; 0.5 keeps
 * both full to the centre; near 1 is a cut at the far end, for scratching. Past that point the
 * deck fades along a quarter cosine, so it eases out rather than dropping at the end.
 */
export function crossfade(position: number, curve = DEFAULT_CURVE): [a: number, b: number] {
  const x = Math.min(1, Math.max(0, position))
  const hold = Math.min(CURVE_MAX, Math.max(0, curve))
  /** Level of a deck `away` from its own end, 0 there to 1 at the other end. */
  const fade = (away: number): number => {
    const t = (away - hold) / (1 - hold)
    return t <= 0 ? 1 : Math.cos((Math.min(1, t) * Math.PI) / 2)
  }
  return [fade(x), fade(1 - x)]
}

export function faderPosition(position: number): number {
  const x = Math.min(1, Math.max(0, position))
  return Math.abs(x - FADER_CENTRE) < CENTRE_SNAP ? FADER_CENTRE : x
}

/**
 * Where on the fade the bass swaps, as fader travel from where the fade began: it starts moving
 * after `start` and is done by `end`.
 */
export interface SwapRange {
  start: number
  end: number
}

/** The swap's range before it could be set: over the first half of the fade, done by the centre. */
export const DEFAULT_SWAP_RANGE: SwapRange = { start: 0, end: 0.5 }
/** The shortest swap, so the bass is never cut over in a click. */
export const MIN_SWAP_TRAVEL = 0.05

/** `range` within the travel, at least `MIN_SWAP_TRAVEL` long. */
export function swapRange(range: SwapRange): SwapRange {
  const start = Math.min(1 - MIN_SWAP_TRAVEL, Math.max(0, range.start))
  return { start, end: Math.min(1, Math.max(start + MIN_SWAP_TRAVEL, range.end)) }
}

/**
 * Where a bass swap stands, for swapping the bass the way a DJ does on the EQs rather than by
 * fader position alone: the deck being faded away from gives its bass up as the fader travels
 * through the swap range, measured from where the fade began.
 *
 * Which deck is outgoing depends on which way the fader is moving, so the fade begins wherever
 * the fader last turned round. Turning round mid-swap swaps the bass back from where it stands,
 * after the same start travel and at the same rate. The fader's position also bounds the swap
 * (see `bassBounds`), so it always ends clean at either side. The bass never jumps, whatever the
 * fader does.
 */
export interface BassSwap {
  /** Deck B's share of the bass, 0 all A's to 1 all B's. Deck A has the rest. */
  share: number
  /** The fader's position at the last move. */
  position: number
  /** Where the fader last turned round, and B's share there. */
  turn: number
  from: number
  /** Which way the fader is moving: 1 towards B, −1 towards A, 0 not yet known. */
  direction: -1 | 0 | 1
}

/**
 * The swap with the fader at `position` and nothing yet known about direction: the nearer deck
 * has the bass, as much as `bassBounds` allows.
 */
export function startBassSwap(position: number, range: SwapRange = DEFAULT_SWAP_RANGE): BassSwap {
  const [low, high] = bassBounds(position, range)
  return rebaseBassSwap(Math.min(high, Math.max(low, position <= 0.5 ? 0 : 1)), position)
}

/** A swap holding `share` with the fader at `position`, whichever way it moves next. */
export function rebaseBassSwap(share: number, position: number): BassSwap {
  return { share, position, turn: position, from: share, direction: 0 }
}

/**
 * The least and most of the bass deck B can have with the fader at `position`, whichever way it
 * has moved. One edge is a fade from A, swapping over `range` from the left; the other a fade
 * from B, swapping over `range` from the right. Both meet at the ends, so the fader fully to one
 * side always leaves that deck with all the bass, however it was wiggled on the way.
 */
export function bassBounds(position: number, range: SwapRange = DEFAULT_SWAP_RANGE): [low: number, high: number] {
  const { start, end } = swapRange(range)
  const done = (away: number): number => Math.min(1, Math.max(0, (away - start) / (end - start)))
  const fromA = done(position)
  const fromB = 1 - done(1 - position)
  return [Math.min(fromA, fromB), Math.max(fromA, fromB)]
}

/**
 * The swap after the fader moves from where it was in `swap` to `position`: on from where it
 * stood by the fader's travel since it last turned round, kept within `bassBounds`.
 */
export function bassSwap(swap: BassSwap, position: number, range: SwapRange = DEFAULT_SWAP_RANGE): BassSwap {
  const [low, high] = bassBounds(position, range)
  const moved = position - swap.position
  if (moved === 0) return { ...swap, share: Math.min(high, Math.max(low, swap.share)) }
  const direction = moved > 0 ? 1 : -1
  const turned = direction !== swap.direction
  const turn = turned ? swap.position : swap.turn
  const from = turned ? swap.share : swap.from
  const { start, end } = swapRange(range)
  const swapped = Math.max(0, Math.abs(position - turn) - start) / (end - start)
  const share = Math.min(high, Math.max(low, from + direction * swapped))
  return { share, position, turn, from, direction }
}

/**
 * The low band's level for a deck with `share` of the bass: a quarter sine, so two decks' basses
 * part way through a swap keep the power of one. At half each, each is at 0.71 (−3 dB).
 */
export function bassLevel(share: number): number {
  return Math.sin((Math.min(1, Math.max(0, share)) * Math.PI) / 2)
}

/** Below this level a deck counts as silent: about -40 dB. */
export const AUDIBLE_GAIN = 0.01

/** The one deck heard (0 for A, 1 for B): playing and not faded out. Null when neither or both are. */
export function soleAudible(playing: [boolean, boolean], gains: [number, number]): 0 | 1 | null {
  const heard = [0, 1].filter((i) => playing[i] && gains[i] >= AUDIBLE_GAIN) as (0 | 1)[]
  return heard.length === 1 ? heard[0] : null
}
