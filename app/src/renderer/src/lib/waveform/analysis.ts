/** Drawing rekordbox-style analysis data: an RGB waveform, and 3-band waveforms. */
import { BANDS_LOW_TO_HIGH } from './colors'
import {
  bandSource,
  byteLevel,
  columns,
  fitColumns,
  paintColumns,
  packColor,
  pixelGrid,
  readBandLevel,
  rgbPeakIndex
} from './columns'
import { bandFall } from './fall'

/** Overview peaks stop this far short of the top of the waveform window. */
export const OVERVIEW_TOP_PAD = 0.1

/** Entries `from`–`to` of an RGB waveform across `pw` pixels. */
export function drawAnalysisRgb(
  ctx: CanvasRenderingContext2D,
  heights: Uint8Array,
  colors: Uint8Array,
  from: number,
  to: number,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center'
): void {
  const n = heights.length
  const grid = pixelGrid(from, to, pw)
  if (!grid || n < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph * (1 - OVERVIEW_TOP_PAD)
  for (let x = 0; x < pw; x++) {
    const start = (first + x) * per
    const peak = rgbPeakIndex(heights, n, start, start + per)
    if (peak < 0) {
      amp[x] = NaN
      color[x] = -1
      continue
    }
    amp[x] = Math.max(1, Math.round(byteLevel(heights[peak]) * reach))
    color[x] = packColor(colors, peak)
  }
  paintColumns(ctx, pw, ph, anchor, (x) => {
    const c = color[x]
    return `rgb(${(c >> 16) & 255},${(c >> 8) & 255},${c & 255})`
  })
}

/**
 * Entries `from`–`to` of interleaved low/mid/high bands, each band layered over the one below.
 * `stride` reads every so-many entries as one; `fallBlocks` lets each level fall over that many.
 */
export function drawAnalysisBands(
  ctx: CanvasRenderingContext2D,
  bands: Uint8Array,
  from: number,
  to: number,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center',
  stride = 1,
  fallBlocks = 0
): void {
  // The fallen bands are worked out at `stride` already, so they are read entry by entry.
  const fallen = fallBlocks >= 1 ? bandFall(bands, stride, fallBlocks) : null
  const source = fallen ? bandSource(fallen, 1) : bandSource(bands, stride)
  const grid = pixelGrid(from, to, pw)
  if (!grid || source.count < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph
  for (const layer of BANDS_LOW_TO_HIGH) {
    const fill = layer.color
    for (let x = 0; x < pw; x++) {
      const start = (first + x) * per
      const level = readBandLevel(source, layer.band, start, start + per, fallen != null)
      const empty = Number.isNaN(level)
      amp[x] = empty ? NaN : level * reach
      color[x] = empty ? -1 : 1
    }
    paintColumns(ctx, pw, ph, anchor, () => fill, true)
  }
}

/** Per-pixel buffers for `drawStackedBands`, shared between draws and only grown. */
let stackedLevels = [new Float32Array(0), new Float32Array(0), new Float32Array(0)]
let stackedFloor = new Float32Array(0)

/** The whole track's bands stacked from the bottom, averaged per pixel, as the overview shows them. */
export function drawStackedBands(ctx: CanvasRenderingContext2D, bands: Uint8Array, pw: number, ph: number): void {
  const length = bands.length / 3
  if (stackedFloor.length < pw) {
    stackedLevels = [new Float32Array(pw), new Float32Array(pw), new Float32Array(pw)]
    stackedFloor = new Float32Array(pw)
  }
  const level = stackedLevels
  const floor0 = stackedFloor
  let peak = 1
  for (let x = 0; x < pw; x++) {
    const a = Math.floor((x * length) / pw)
    const b = Math.max(a + 1, Math.floor(((x + 1) * length) / pw))
    let sum = 0
    for (let band = 0; band < 3; band++) {
      let bandSum = 0
      for (let i = a; i < b; i++) bandSum += bands[i * 3 + band]
      sum += bandSum / (b - a)
      level[band][x] = sum
    }
    if (sum > peak) peak = sum
  }
  const scale = (ph * (1 - OVERVIEW_TOP_PAD)) / peak
  for (let band = 0; band < 3; band++) {
    const floor = band === 0 ? floor0 : level[band - 1]
    const ceil = level[band]
    ctx.fillStyle = BANDS_LOW_TO_HIGH[band].color
    ctx.beginPath()
    ctx.moveTo(0, ph - floor[0] * scale)
    for (let x = 0; x < pw; x++) ctx.lineTo(x + 0.5, ph - ceil[x] * scale)
    for (let x = pw - 1; x >= 0; x--) ctx.lineTo(x + 0.5, ph - floor[x] * scale)
    ctx.closePath()
    ctx.fill()
  }
}
