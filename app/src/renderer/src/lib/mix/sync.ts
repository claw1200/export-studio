import { MAX_TEMPO as MAX_RATE, MIN_TEMPO as MIN_RATE } from '../../../../shared/engine'

/** The rate that moves a deck playing at `rate` by `step` BPM, its music being at `ownBpm`. */
export function stepTempo(rate: number, ownBpm: number, step: number): number | null {
  if (!(ownBpm > 0) || !(rate > 0)) return null
  const stepped = rate + step / ownBpm
  return Math.max(MIN_RATE, Math.min(MAX_RATE, stepped))
}
