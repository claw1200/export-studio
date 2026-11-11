import { getSetting, setSetting } from '../settings'
import { isBandList, isHex } from './bandStyle.svelte'

export const DEFAULT_RGB_GRADIENT: readonly string[] = ['#0054ff', '#02cff7', '#ffffff']

const KEY = 'waveformRgbGradient'

function stored(): string[] {
  let saved: unknown = null
  try {
    saved = JSON.parse(getSetting(KEY) ?? 'null')
  } catch {
    saved = null
  }
  return isBandList(saved, isHex) ? saved : [...DEFAULT_RGB_GRADIENT]
}

/** The colours, low band to high, that an RGB waveform coloured from the audio runs through. */
class RgbGradient {
  private stops = $state(stored())

  get stopsLowToHigh(): readonly string[] {
    return this.stops
  }

  setStop(band: number, color: string): void {
    const hex = color.toLowerCase()
    if (!isHex(hex)) return
    this.stops[band] = hex
    setSetting(KEY, JSON.stringify(this.stops))
  }
}

export const rgbGradient = new RgbGradient()
