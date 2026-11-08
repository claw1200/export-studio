import { MAX_TEMPO as MAX_RATE, MIN_TEMPO as MIN_RATE } from '../../../../shared/engine'

/** The speed that moves a deck playing at `currentBpm` by `step` BPM, its music being `ownBpm`. */
export function stepTempo(currentBpm: number, ownBpm: number, step: number): number | null {
  if (!(ownBpm > 0) || !(currentBpm > 0)) return null
  const rate = (currentBpm + step) / ownBpm
  return Math.max(MIN_RATE, Math.min(MAX_RATE, rate))
}
