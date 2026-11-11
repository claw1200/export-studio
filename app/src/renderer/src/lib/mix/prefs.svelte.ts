import type { SwapRange } from '../crossfade'
import { getSetting, setSetting } from '../settings'
import type { KeyMatch } from './harmonic'

export type CurveId = 'smooth' | 'normal' | 'sharp'
export type SwapTimingId = 'early' | 'centre' | 'late'

/** Crossfader curves, by how far each deck holds full level as the fader moves away. See `crossfade`. */
export const CURVES: { id: CurveId; label: string; curve: number }[] = [
  { id: 'smooth', label: 'Smooth', curve: 0 },
  { id: 'normal', label: 'Normal', curve: 0.5 },
  { id: 'sharp', label: 'Sharp', curve: 0.75 }
]

/** Where on the fade the bass swaps. See `bassSwap`. */
export const SWAP_TIMINGS: { id: SwapTimingId; label: string; range: SwapRange }[] = [
  { id: 'early', label: 'Early', range: { start: 0, end: 0.5 } },
  { id: 'centre', label: 'Centre', range: { start: 0.25, end: 0.75 } },
  { id: 'late', label: 'Late', range: { start: 0.5, end: 1 } }
]

export type KeyHighlightId = 'off' | KeyMatch

/** The loosest key match the browser highlights against the master deck. See `keyMatch`. */
export const KEY_HIGHLIGHTS: { id: KeyHighlightId; label: string }[] = [
  { id: 'off', label: 'Off' },
  { id: 'same', label: 'Same' },
  { id: 'relative', label: 'Relative' },
  { id: 'adjacent', label: '±1' },
  { id: 'two', label: '+2' },
  { id: 'boost', label: '+7' }
]

export const DEFAULT_CURVE_ID: CurveId = 'normal'
export const DEFAULT_SWAP_TIMING_ID: SwapTimingId = 'centre'
export const DEFAULT_KEY_HIGHLIGHT_ID: KeyHighlightId = 'adjacent'
export const DEFAULT_AUTO_MASTER = true

function stored<T extends string>(key: string, ids: { id: T }[], fallback: T): T {
  const raw = getSetting(key)
  return ids.find((o) => o.id === raw)?.id ?? fallback
}

/** How mix mode's crossfader, bass swap, master deck and key highlighting behave, kept between sessions. */
export class MixPrefs {
  curveId = $state<CurveId>(stored('mix.curve', CURVES, DEFAULT_CURVE_ID))
  swapTimingId = $state<SwapTimingId>(stored('mix.swapTiming', SWAP_TIMINGS, DEFAULT_SWAP_TIMING_ID))
  /** The deck that alone has sound becomes the master. See `Mixer.makeSoleAudibleDeckMaster`. */
  autoMaster = $state((getSetting('mix.autoMaster') ?? String(DEFAULT_AUTO_MASTER)) !== 'false')
  keyHighlightId = $state<KeyHighlightId>(stored('mix.keyHighlight', KEY_HIGHLIGHTS, DEFAULT_KEY_HIGHLIGHT_ID))

  get curve(): number {
    return CURVES.find((c) => c.id === this.curveId)!.curve
  }

  get swapRange(): SwapRange {
    return SWAP_TIMINGS.find((t) => t.id === this.swapTimingId)!.range
  }

  /** The loosest match highlighted, or null when highlighting is off. */
  get keyCutoff(): KeyMatch | null {
    return this.keyHighlightId === 'off' ? null : this.keyHighlightId
  }

  setCurve(id: CurveId): void {
    this.curveId = id
    setSetting('mix.curve', id)
  }

  setSwapTiming(id: SwapTimingId): void {
    this.swapTimingId = id
    setSetting('mix.swapTiming', id)
  }

  setKeyHighlight(id: KeyHighlightId): void {
    this.keyHighlightId = id
    setSetting('mix.keyHighlight', id)
  }

  setAutoMaster(on: boolean): void {
    this.autoMaster = on
    setSetting('mix.autoMaster', String(on))
  }
}
