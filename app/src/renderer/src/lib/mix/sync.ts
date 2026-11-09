import { MAX_TEMPO as MAX_RATE, MIN_TEMPO as MIN_RATE } from '../../../../shared/engine'

export function stepTempo(rate: number, trackBpm: number, bpmStep: number): number | null {
  if (!(trackBpm > 0) || !(rate > 0)) return null
  const stepped = rate + bpmStep / trackBpm
  return Math.max(MIN_RATE, Math.min(MAX_RATE, stepped))
}
