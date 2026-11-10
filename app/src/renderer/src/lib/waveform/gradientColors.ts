import { heldBandPeaks } from './fall'

type Rgb = readonly [red: number, green: number, blue: number]

export function hexToRgb(hex: string): Rgb {
  const packed = parseInt(hex.slice(1), 16)
  return [(packed >> 16) & 255, (packed >> 8) & 255, packed & 255]
}

/** 0 when all of the sound is low, 1 when all of it is high; silence counts as low. */
export function lowToHighBalance(low: number, mid: number, high: number): number {
  const total = low + mid + high
  return total > 0 ? (mid / 2 + high) / total : 0
}

function writeGradientColor(stopsLowToHigh: readonly Rgb[], balance: number, out: Uint8Array, offset: number): void {
  const scaled = balance * (stopsLowToHigh.length - 1)
  const lower = Math.min(stopsLowToHigh.length - 2, Math.floor(scaled))
  const fraction = scaled - lower
  const from = stopsLowToHigh[lower]
  const to = stopsLowToHigh[lower + 1]
  for (let channel = 0; channel < 3; channel++)
    out[offset + channel] = Math.round(from[channel] + (to[channel] - from[channel]) * fraction)
}

interface Colored {
  key: string
  colors: Uint8Array
}

const coloredByBands = new WeakMap<Uint8Array, Colored>()

/**
 * Red, green and blue bytes for each of `entries` waveform entries, laid out like rekordbox's RGB
 * colours, from where each entry's band peaks sit between the gradient's stops. Null without bands.
 */
export function gradientColors(
  bands: Uint8Array,
  bandsPerSecond: number,
  entriesPerSecond: number,
  entries: number,
  stopsLowToHigh: readonly string[]
): Uint8Array | null {
  const key = `${bandsPerSecond}|${entriesPerSecond}|${entries}|${stopsLowToHigh.join()}`
  const kept = coloredByBands.get(bands)
  if (kept?.key === key) return kept.colors
  const stride = Math.max(1, bandsPerSecond / entriesPerSecond)
  const blocksPerSecond = bandsPerSecond / stride
  const peaks = heldBandPeaks(bands, stride, blocksPerSecond)
  const blocks = peaks.length / 3
  if (blocks < 1 || entries < 1) return null
  const stops = stopsLowToHigh.map(hexToRgb)
  const colors = new Uint8Array(entries * 3)
  for (let entry = 0; entry < entries; entry++) {
    const block = Math.min(blocks - 1, Math.floor((entry * blocksPerSecond) / entriesPerSecond)) * 3
    const balance = lowToHighBalance(peaks[block], peaks[block + 1], peaks[block + 2])
    writeGradientColor(stops, balance, colors, entry * 3)
  }
  coloredByBands.set(bands, { key, colors })
  return colors
}
