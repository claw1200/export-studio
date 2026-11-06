/** How close to the middle snaps to it, so the centre is easy to find by hand. */
export const CENTRE_SNAP = 0.02

/**
 * Levels for a crossfader at `position`, 0 all A to 1 all B. Both decks are at full level (0 dB)
 * from the centre outwards to their own side; moving away from a deck past the centre fades only
 * that deck, along a quarter cosine, so it eases out rather than dropping at the end.
 */
export function crossfade(position: number): [a: number, b: number] {
  const x = Math.min(1, Math.max(0, position))
  /** Level of a deck the fader has moved `away` from, 0 at the centre to 1 at the far end. */
  const fade = (away: number): number => (away <= 0 ? 1 : Math.cos((Math.min(1, away) * Math.PI) / 2))
  return [fade((x - 0.5) * 2), fade((0.5 - x) * 2)]
}

/** A dragged position, clamped and pulled onto the centre when it is close. */
export function faderPosition(position: number): number {
  const x = Math.min(1, Math.max(0, position))
  return Math.abs(x - 0.5) < CENTRE_SNAP ? 0.5 : x
}

/** Fader travel over which the bass swaps from one deck to the other. */
export const SWAP_TRAVEL = 0.5

/**
 * Where a bass swap stands, for swapping the bass the way a DJ does on the EQs rather than by
 * fader position alone: the deck being faded away from gives its bass up over the first half of
 * the fade, so by the centre the incoming deck has all of it and the outgoing none.
 *
 * Which deck is outgoing depends on which way the fader is moving, so the swap is the fader's
 * position with `SWAP_TRAVEL` of slack in it. `anchor` is the trailing edge of that slack: moving
 * towards B drags it along behind the fader once the swap is complete, and moving back towards A
 * swaps the bass back from wherever the fader turns round. The bass never jumps, whatever the
 * fader does.
 */
export interface BassSwap {
  anchor: number
  /** Deck B's share of the bass, 0 all A's to 1 all B's. Deck A has the rest. */
  share: number
}

/** The swap with the fader at `position` and nothing yet known about direction: the nearer deck has the bass. */
export function startBassSwap(position: number): BassSwap {
  return bassSwap(position <= 0.5 ? position : position - SWAP_TRAVEL, position)
}

/** The swap after the fader moves to `position` from wherever left `anchor`. */
export function bassSwap(anchor: number, position: number): BassSwap {
  const at = Math.min(1, Math.max(0, anchor, position - SWAP_TRAVEL), position)
  return { anchor: at, share: (position - at) / SWAP_TRAVEL }
}

/**
 * The low band's level for a deck with `share` of the bass: a quarter sine, so two decks' basses
 * part way through a swap keep the power of one. At half each, each is at 0.71 (−3 dB).
 */
export function bassLevel(share: number): number {
  return Math.sin((Math.min(1, Math.max(0, share)) * Math.PI) / 2)
}
