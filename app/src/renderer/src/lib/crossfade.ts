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
