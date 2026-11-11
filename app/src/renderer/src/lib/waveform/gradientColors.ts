import { Lru } from '../lru'
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

const PENDING_GREY = 180

const pendingByLength = new Lru<number, Uint8Array>(8)

/** Grey for each of `entries` waveform entries, shown until the gradient colours can be worked out. */
export function pendingColors(entries: number): Uint8Array {
  const kept = pendingByLength.get(entries)
  if (kept) return kept
  const colors = new Uint8Array(entries * 3).fill(PENDING_GREY)
  pendingByLength.set(entries, colors)
  return colors
}

interface Colored {
  key: string
  colors: Uint8Array
}

const coloredByBands = new WeakMap<Uint8Array, Colored>()

/** Balances this rare at either end are let run off the gradient, so the rest spread across it. */
const BALANCE_OUTLIER_SHARE = 0.02
/** A track whose balances spread less than this keeps them as they are, rather than magnifying noise. */
const MIN_BALANCE_SPREAD = 0.05

function balanceAtShare(sortedBalances: Float32Array, share: number): number {
  return sortedBalances[Math.round(share * (sortedBalances.length - 1))]
}

/** Spreads the track's own range of balances across the whole gradient. */
export function stretchBalances(balances: Float32Array): void {
  const sorted = balances.slice().sort()
  const lowest = balanceAtShare(sorted, BALANCE_OUTLIER_SHARE)
  const highest = balanceAtShare(sorted, 1 - BALANCE_OUTLIER_SHARE)
  const spread = highest - lowest
  if (spread < MIN_BALANCE_SPREAD) return
  for (let i = 0; i < balances.length; i++) balances[i] = Math.min(1, Math.max(0, (balances[i] - lowest) / spread))
}

function entryBalances(
  bands: Uint8Array,
  bandsPerSecond: number,
  entriesPerSecond: number,
  entries: number
): Float32Array | null {
  const stride = Math.max(1, bandsPerSecond / entriesPerSecond)
  const blocksPerSecond = bandsPerSecond / stride
  const peaks = heldBandPeaks(bands, stride, blocksPerSecond)
  const blocks = peaks.length / 3
  if (blocks < 1 || entries < 1) return null
  const balances = new Float32Array(entries)
  for (let entry = 0; entry < entries; entry++) {
    const block = Math.min(blocks - 1, Math.floor((entry * blocksPerSecond) / entriesPerSecond)) * 3
    balances[entry] = lowToHighBalance(peaks[block], peaks[block + 1], peaks[block + 2])
  }
  return balances
}

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
  const balances = entryBalances(bands, bandsPerSecond, entriesPerSecond, entries)
  if (!balances) return null
  stretchBalances(balances)
  const stops = stopsLowToHigh.map(hexToRgb)
  const colors = new Uint8Array(entries * 3)
  for (let entry = 0; entry < entries; entry++) writeGradientColor(stops, balances[entry], colors, entry * 3)
  coloredByBands.set(bands, { key, colors })
  return colors
}
