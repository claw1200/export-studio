import { MAX_TEMPO, MIN_TEMPO } from '../../../../shared/engine'
import type { Deck } from './deck.svelte'
import { storedFlag, storeFlag } from './stored'

export class TempoControl {
  lockedToMaster = $state(false)

  constructor(private deck: Deck) {
    if (storedFlag('masterTempo')) deck.setMasterTempo(true)
  }

  get rate(): number {
    return this.deck.tempo
  }

  get percent(): number {
    return (this.deck.tempo - 1) * 100
  }

  get masterTempo(): boolean {
    return this.deck.masterTempo
  }

  reset(): void {
    if (this.lockedToMaster) return
    this.deck.setTempo(1)
  }

  setMasterTempo(on: boolean): void {
    this.deck.setMasterTempo(on)
  }

  setMasterTempoAndDefault(on: boolean): void {
    this.setMasterTempo(on)
    storeFlag('masterTempo', on)
  }

  setPercent(percent: number): void {
    this.setRate(1 + percent / 100)
  }

  setRate(rate: number): void {
    if (this.lockedToMaster || !(rate > 0)) return
    this.deck.setTempo(Math.max(MIN_TEMPO, Math.min(MAX_TEMPO, rate)))
  }

  get keyShift(): number {
    return this.deck.keyShift
  }

  setKeyShift(semitones: number): void {
    this.deck.setKeyShift(semitones)
  }
}
