import { getSetting, setSetting } from '../settings'
import { WaveformPrefs } from './display.svelte'
import { storedFlag, storeFlag } from './stored'

export type CuePlacement = 'playhead' | 'cuePoint'

/** Deck preferences that hold for every deck at once, kept between sessions. */
export class DeckPrefs {
  quantise = $state(storedFlag('quantise', true))
  cuePlacement = $state<CuePlacement>(getSetting('cuePlacement') === 'playhead' ? 'playhead' : 'cuePoint')
  readonly waveform = new WaveformPrefs()

  setQuantise(on: boolean): void {
    this.quantise = on
    storeFlag('quantise', on)
  }

  setCuePlacement(placement: CuePlacement): void {
    this.cuePlacement = placement
    setSetting('cuePlacement', placement)
  }
}
