import { Lru } from '../lru'
import { logicalCount } from './columns'

interface Peaks {
  stride: number
  data: Uint8Array
}

interface Fallen extends Peaks {
  timeConstantBlocks: number
}

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

function cachedPeaks(bands: Uint8Array, stride: number): Uint8Array {
  const kept = peaksByTrack.get(bands)
  if (kept && kept.stride === stride) return kept.data
  const data = peaksPerStride(bands, stride)
  peaksByTrack.set(bands, { stride, data })
  return data
}

export function bandFall(bands: Uint8Array, stride: number, timeConstantBlocks: number): Uint8Array {
  const kept = fallenByTrack.get(bands)
  if (kept && kept.stride === stride && kept.timeConstantBlocks === timeConstantBlocks) return kept.data
  const data = decayAfterPeaks(cachedPeaks(bands, stride), timeConstantBlocks)
  fallenByTrack.set(bands, { stride, timeConstantBlocks, data })
  return data
}
