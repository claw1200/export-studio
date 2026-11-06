import { MAX_TEMPO as MAX_RATE, MIN_TEMPO as MIN_RATE } from '../../../../shared/engine'

/**
 * Tempo and beat matching for two decks, kept free of the decks themselves so it can be tested on
 * its own. BPMs are beats per minute; rates are playback speed as a multiple of the file's.
 */

export interface SyncMatch {
  /** Playback speed for the following deck. */
  rate: number
  /**
   * Beats the following deck plays for each of the master's: 1 at the same tempo, 2 when it plays
   * at double time, ½ at half time.
   */
  beatsPerMasterBeat: number
}

/**
 * The speed for a deck whose music is at `ownBpm` to keep time with a master playing at
 * `masterBpm`. Half and double time count as in time, so the deck takes whichever of the three
 * is nearest its own speed: an 87 BPM track follows a 174 BPM master at its own pace.
 */
export function matchTempo(masterBpm: number, ownBpm: number): SyncMatch | null {
  if (!(masterBpm > 0) || !(ownBpm > 0)) return null
  let best: SyncMatch | null = null
  for (const beatsPerMasterBeat of [1, 2, 0.5]) {
    const rate = (masterBpm * beatsPerMasterBeat) / ownBpm
    if (rate < MIN_RATE || rate > MAX_RATE) continue
    if (!best || Math.abs(Math.log(rate)) < Math.abs(Math.log(best.rate)) - 1e-9) best = { rate, beatsPerMasterBeat }
  }
  return best
}

/** The fractional part of a beat position, 0 up to 1, the same either side of zero. */
function phaseOf(beats: number): number {
  return beats - Math.floor(beats)
}

/**
 * How far the following deck is off the master's beat, in its own beats, from -½ to ½: positive
 * when it is ahead. `masterBeat` and `ownBeat` are fractional beat positions along each grid.
 */
export function beatOffset(masterBeat: number, ownBeat: number, beatsPerMasterBeat = 1): number {
  let offset = phaseOf(ownBeat) - phaseOf(masterBeat * beatsPerMasterBeat)
  if (offset > 0.5) offset -= 1
  else if (offset <= -0.5) offset += 1
  return offset
}

/** The speed that moves a deck playing at `currentBpm` by `step` BPM, its music being `ownBpm`. */
export function stepTempo(currentBpm: number, ownBpm: number, step: number): number | null {
  if (!(ownBpm > 0) || !(currentBpm > 0)) return null
  const rate = (currentBpm + step) / ownBpm
  return Math.max(MIN_RATE, Math.min(MAX_RATE, rate))
}
