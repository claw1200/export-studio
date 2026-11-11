import { Lru } from '../lru'
import { logicalCount } from './columns'

export interface BandFall {
  blocksPerSecond: number
  timeConstantBlocks: number
}

interface Peaks {
  stride: number
  blocksPerSecond: number
  data: Uint8Array
}

interface Fallen extends Peaks {
  timeConstantBlocks: number
}

/**
 * The lowest frequency each band carries. Entries are far shorter than a bass cycle, so they
 * trace the rectified wave; holding a level for half its cycle stops each ripple reading as a hit.
 */
const LOWEST_HZ_LOW_TO_HIGH = [40, 200, 2000]

/** Every deck on screen, with room to switch back. */
const TRACKS_KEPT = 4

const peaksByTrack = new Lru<Uint8Array, Peaks>(TRACKS_KEPT)
const fallenByTrack = new Lru<Uint8Array, Fallen>(TRACKS_KEPT)

function peaksPerStride(bands: Uint8Array, stride: number): Uint8Array {
  if (stride <= 1) return bands
  const entries = bands.length / 3
  const blocks = logicalCount(entries, stride)
  const out = new Uint8Array(blocks * 3)
  for (let block = 0; block < blocks; block++) {
    const first = Math.min(entries, Math.floor(block * stride))
    const end = Math.min(entries, Math.max(first + 1, Math.ceil((block + 1) * stride)))
    let low = 0
    let mid = 0
    let high = 0
    for (let i = first * 3; i < end * 3; i += 3) {
      if (bands[i] > low) low = bands[i]
      if (bands[i + 1] > mid) mid = bands[i + 1]
      if (bands[i + 2] > high) high = bands[i + 2]
    }
    out[block * 3] = low
    out[block * 3 + 1] = mid
    out[block * 3 + 2] = high
  }
  return out
}

/** Van Herk/Gil-Werman: the max of each `reach`-either-side window from running maxima within window-sized chunks. */
export function slidingMax(column: Uint8Array, reach: number): Uint8Array {
  const width = reach * 2 + 1
  const silencePadded = new Uint8Array(column.length + reach * 2)
  silencePadded.set(column, reach)
  const n = silencePadded.length
  const forward = new Uint8Array(n)
  const backward = new Uint8Array(n)
  for (let chunk = 0; chunk < n; chunk += width) {
    const end = Math.min(n, chunk + width)
    let running = 0
    for (let i = chunk; i < end; i++) forward[i] = running = Math.max(running, silencePadded[i])
    running = 0
    for (let i = end - 1; i >= chunk; i--) backward[i] = running = Math.max(running, silencePadded[i])
  }
  const out = new Uint8Array(column.length)
  for (let i = 0; i < column.length; i++) out[i] = Math.max(backward[i], forward[i + width - 1])
  return out
}

function holdOverHalfCycle(peaks: Uint8Array, blocksPerSecond: number): Uint8Array {
  const blocks = peaks.length / 3
  const out = new Uint8Array(peaks.length)
  const column = new Uint8Array(blocks)
  for (let band = 0; band < 3; band++) {
    const quarterCycleBlocks = Math.round(blocksPerSecond / LOWEST_HZ_LOW_TO_HIGH[band] / 4)
    for (let block = 0; block < blocks; block++) column[block] = peaks[block * 3 + band]
    const held = quarterCycleBlocks > 0 ? slidingMax(column, quarterCycleBlocks) : column
    for (let block = 0; block < blocks; block++) out[block * 3 + band] = held[block]
  }
  return out
}

function decayAfterPeaks(peaks: Uint8Array, timeConstantBlocks: number): Uint8Array {
  const decayPerBlock = Math.exp(-1 / timeConstantBlocks)
  const out = new Uint8Array(peaks.length)
  let low = 0
  let mid = 0
  let high = 0
  for (let i = 0; i < peaks.length; i += 3) {
    low = Math.max(peaks[i], low * decayPerBlock)
    mid = Math.max(peaks[i + 1], mid * decayPerBlock)
    high = Math.max(peaks[i + 2], high * decayPerBlock)
    out[i] = Math.round(low)
    out[i + 1] = Math.round(mid)
    out[i + 2] = Math.round(high)
  }
  return out
}

export function heldBandPeaks(bands: Uint8Array, stride: number, blocksPerSecond: number): Uint8Array {
  return holdOverHalfCycle(peaksPerStride(bands, stride), blocksPerSecond)
}

function cachedHeldPeaks(bands: Uint8Array, stride: number, blocksPerSecond: number): Uint8Array {
  const kept = peaksByTrack.get(bands)
  if (kept && kept.stride === stride && kept.blocksPerSecond === blocksPerSecond) return kept.data
  const data = heldBandPeaks(bands, stride, blocksPerSecond)
  peaksByTrack.set(bands, { stride, blocksPerSecond, data })
  return data
}

export function bandFall(bands: Uint8Array, stride: number, fall: BandFall): Uint8Array {
  const { blocksPerSecond, timeConstantBlocks } = fall
  const kept = fallenByTrack.get(bands)
  const unchanged =
    kept &&
    kept.stride === stride &&
    kept.blocksPerSecond === blocksPerSecond &&
    kept.timeConstantBlocks === timeConstantBlocks
  if (unchanged) return kept.data
  const data = decayAfterPeaks(cachedHeldPeaks(bands, stride, blocksPerSecond), timeConstantBlocks)
  fallenByTrack.set(bands, { stride, blocksPerSecond, timeConstantBlocks, data })
  return data
}
