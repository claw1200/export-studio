/** Drawing rekordbox-style analysis data: an RGB waveform, and 3-band waveforms. */
import { shapeBandLevel } from './bandCurves'
import { BAND_NAMES_LOW_TO_HIGH, bandStyle } from './bandStyle.svelte'
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

let bandLayers: HTMLCanvasElement | null = null

/**
 * Bands are layered on their own canvas so a translucent band only shows the bands beneath it:
 * where nothing is beneath, a second opaque pass behind it fills it in.
 */
function clearedBandLayers(pw: number, ph: number): CanvasRenderingContext2D {
  bandLayers ??= document.createElement('canvas')
  const width = Math.ceil(pw)
  const height = Math.ceil(ph)
  if (bandLayers.width !== width || bandLayers.height !== height) {
    bandLayers.width = width
    bandLayers.height = height
  }
  const layers = bandLayers.getContext('2d')!
  layers.clearRect(0, 0, width, height)
  return layers
}

/**
 * Entries `from`–`to` of interleaved low/mid/high bands, each band layered over the one below.
 * `stride` reads every so-many entries as one; `fallBlocks` is how slowly each level eases down after a peak.
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
  const fallen = fallBlocks >= 1 ? bandFall(bands, stride, fallBlocks) : null
  const source = fallen ? bandSource(fallen, 1) : bandSource(bands, stride)
  const grid = pixelGrid(from, to, pw)
  if (!grid || source.count < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph
  const layers = clearedBandLayers(pw, ph)
  const { colors, layeredOpacities, blend } = bandStyle
  for (let band = 0; band < BAND_NAMES_LOW_TO_HIGH.length; band++) {
    const fill = colors[band]
    for (let x = 0; x < pw; x++) {
      const start = (first + x) * per
      const level = readBandLevel(source, band, start, start + per, fallen != null)
      const empty = Number.isNaN(level)
      amp[x] = empty ? NaN : shapeBandLevel(band, level) * reach
      color[x] = empty ? -1 : 1
    }
    layers.globalCompositeOperation = blend
    layers.globalAlpha = layeredOpacities[band]
    paintColumns(layers, pw, ph, anchor, () => fill, true)
    layers.globalCompositeOperation = 'destination-over'
    layers.globalAlpha = 1
    paintColumns(layers, pw, ph, anchor, () => fill, true)
  }
  ctx.drawImage(layers.canvas, 0, 0)
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
  let peak = byteLevel(1)
  for (let x = 0; x < pw; x++) {
    const a = Math.floor((x * length) / pw)
    const b = Math.max(a + 1, Math.floor(((x + 1) * length) / pw))
    let sum = 0
    for (let band = 0; band < 3; band++) {
      let bandSum = 0
      for (let i = a; i < b; i++) bandSum += bands[i * 3 + band]
      sum += shapeBandLevel(band, byteLevel(bandSum / (b - a)))
      level[band][x] = sum
    }
    if (sum > peak) peak = sum
  }
  const scale = (ph * (1 - OVERVIEW_TOP_PAD)) / peak
  const { colors } = bandStyle
  for (let band = 0; band < 3; band++) {
    const floor = band === 0 ? floor0 : level[band - 1]
    const ceil = level[band]
    ctx.fillStyle = colors[band]
    ctx.beginPath()
    ctx.moveTo(0, ph - floor[0] * scale)
    for (let x = 0; x < pw; x++) ctx.lineTo(x + 0.5, ph - ceil[x] * scale)
    for (let x = pw - 1; x >= 0; x--) ctx.lineTo(x + 0.5, ph - floor[x] * scale)
    ctx.closePath()
    ctx.fill()
  }
}
