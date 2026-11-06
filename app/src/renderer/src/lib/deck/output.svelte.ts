import { setSetting } from '../settings'
import { clampUnit, storedFlag, storedVolume, storeFlag } from './stored'

/** The master level and mute, kept between sessions. One for the whole engine, whatever the decks. */
export class Output {
  volume = $state(storedVolume())
  muted = $state(storedFlag('muted'))
  /** What unmuting goes back to when the level was dragged to zero. */
  private unmuteVolume = 1

  constructor() {
    this.unmuteVolume = this.volume > 0 ? this.volume : 1
    this.apply(this.muted ? 0 : this.volume)
  }

  setVolume(volume: number): void {
    const v = clampUnit(volume)
    this.volume = v
    if (v > 0) {
      this.unmuteVolume = v
      this.muted = false
    }
    this.apply(this.muted ? 0 : v)
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
    this.apply(0)
    storeFlag('muted', true)
  }

  private apply(gain: number): void {
    window.audio.send({ op: 'volume', gain })
  }
}
