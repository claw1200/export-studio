import { getSetting, setSetting } from '../settings'
import { WaveformPrefs } from './display.svelte'
import { storedFlag, storeFlag } from './stored'

export type CuePlacement = 'playhead' | 'cuePoint'
export type TempoReadout = 'absolute' | 'relative'

/** Deck preferences that hold for every deck at once, kept between sessions. */
export class DeckPrefs {
  quantise = $state(storedFlag('quantise', true))
  cuePlacement = $state<CuePlacement>(getSetting('cuePlacement') === 'playhead' ? 'playhead' : 'cuePoint')
  snapPlayhead = $state(storedFlag('snapPlayhead'))
  unlimitedMemoryCues = $state(storedFlag('unlimitedMemoryCues'))
  tempoReadout = $state<TempoReadout>(getSetting('tempoReadout') === 'relative' ? 'relative' : 'absolute')
  readonly waveform = new WaveformPrefs()

  setQuantise(on: boolean): void {
    this.quantise = on
    storeFlag('quantise', on)
  }

  setSnapPlayhead(on: boolean): void {
    this.snapPlayhead = on
    storeFlag('snapPlayhead', on)
  }

  setUnlimitedMemoryCues(on: boolean): void {
    this.unlimitedMemoryCues = on
    storeFlag('unlimitedMemoryCues', on)
  }

  get developerOptionsAreDefault(): boolean {
    return !this.snapPlayhead && !this.unlimitedMemoryCues && this.waveform.heightScalesAreDefault
  }

  resetDeveloperOptions(): void {
    this.setSnapPlayhead(false)
    this.setUnlimitedMemoryCues(false)
    this.waveform.resetHeightScales()
  }

  setCuePlacement(placement: CuePlacement): void {
    this.cuePlacement = placement
    setSetting('cuePlacement', placement)
  }

  setTempoReadout(readout: TempoReadout): void {
    this.tempoReadout = readout
    setSetting('tempoReadout', readout)
  }
}
