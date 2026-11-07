import type { SwapRange } from '../crossfade'
import { getSetting, setSetting } from '../settings'

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

export const DEFAULT_CURVE_ID: CurveId = 'normal'
export const DEFAULT_SWAP_TIMING_ID: SwapTimingId = 'late'

function stored<T extends string>(key: string, ids: { id: T }[], fallback: T): T {
  const raw = getSetting(key)
  return ids.find((o) => o.id === raw)?.id ?? fallback
}

/** How mix mode's crossfader and bass swap behave, kept between sessions. */
export class MixPrefs {
  curveId = $state<CurveId>(stored('mix.curve', CURVES, DEFAULT_CURVE_ID))
  swapTimingId = $state<SwapTimingId>(stored('mix.swapTiming', SWAP_TIMINGS, DEFAULT_SWAP_TIMING_ID))

  get curve(): number {
    return CURVES.find((c) => c.id === this.curveId)!.curve
  }

  get swapRange(): SwapRange {
    return SWAP_TIMINGS.find((t) => t.id === this.swapTimingId)!.range
  }

  setCurve(id: CurveId): void {
    this.curveId = id
    setSetting('mix.curve', id)
  }

  setSwapTiming(id: SwapTimingId): void {
    this.swapTimingId = id
    setSetting('mix.swapTiming', id)
  }
}
