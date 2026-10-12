import { setSetting } from '../settings'
import type { Deck } from './deck.svelte'
import { clampUnit, storedFlag, storedVolume, storeFlag } from './stored'

/** The master level and mute, kept between sessions. */
export class Output {
  volume = $state(storedVolume())
  muted = $state(storedFlag('muted'))
  /** What unmuting goes back to when the level was dragged to zero. */
  private unmuteVolume = 1

  constructor(private deck: Deck) {
    this.unmuteVolume = this.volume > 0 ? this.volume : 1
    this.deck.setVolume(this.muted ? 0 : this.volume)
  }

  setVolume(volume: number): void {
    const v = clampUnit(volume)
    this.volume = v
    if (v > 0) {
      this.unmuteVolume = v
      this.muted = false
    }
    this.deck.setVolume(this.muted ? 0 : v)
    setSetting('volume', String(v))
    storeFlag('muted', this.muted)
  }

  toggleMute(): void {
    if (this.muted || this.volume === 0) {
      this.muted = false
      this.setVolume(this.unmuteVolume > 0 ? this.unmuteVolume : 1)
      return
    }
    this.muted = true
    this.deck.setVolume(0)
    storeFlag('muted', true)
  }
}
