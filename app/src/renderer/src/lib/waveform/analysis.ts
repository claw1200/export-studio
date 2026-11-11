/** Drawing rekordbox-style analysis data: an RGB waveform, and 3-band waveforms. */
import { shapeBandLevel } from './bandCurves'
import { BAND_NAMES_LOW_TO_HIGH, bandStyle } from './bandStyle.svelte'
import {
  bandSource,
  byteLevel,
  columns,
  cssColor,
  fitColumns,
  paintColumns,
  packColor,
  packRgb,
  pixelGrid,
  readBandLevel,
  rgbPeakIndex
} from './columns'
import { type BandFall, bandFall } from './fall'

/** Overview peaks stop this far short of the top of the waveform window. */
export const OVERVIEW_TOP_PAD = 0.1

function heightInPixels(level: number, reach: number): number {
  return Math.max(1, Math.round(level * reach))
}

/** Entries `from`–`to` of an RGB waveform across `pw` pixels, mirrored about the middle. */
export function drawRgbDetail(
  ctx: CanvasRenderingContext2D,
  heights: Uint8Array,
  colors: Uint8Array,
  from: number,
  to: number,
  pw: number,
  ph: number
): void {
  const n = heights.length
  const grid = pixelGrid(from, to, pw)
  if (!grid || n < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  const { per, first } = grid
  for (let x = 0; x < pw; x++) {
    const start = (first + x) * per
    const peak = rgbPeakIndex(heights, n, start, start + per)
    if (peak < 0) {
      amp[x] = NaN
      color[x] = -1
      continue
    }
    amp[x] = heightInPixels(byteLevel(heights[peak]), ph / 2)
    color[x] = packColor(colors, peak)
  }
  paintColumns(ctx, pw, ph, 'center', (x) => cssColor(color[x]))
}

/**
 * Column levels, scaled to the loudest column, are raised to this power. A whole track squeezed
 * into a few hundred pixels is otherwise loud nearly everywhere; this lets the quieter passages sink.
 */
const OVERVIEW_DYNAMICS_EXPONENT = 1.5

/**
 * The first `entries` of an RGB waveform from the bottom across `pw` pixels. Each column is the
 * average of the entries it covers, coloured by them weighted by height.
 */
export function drawRgbOverview(
  ctx: CanvasRenderingContext2D,
  heights: Uint8Array,
  colors: Uint8Array,
  entries: number,
  pw: number,
  ph: number
): void {
  const n = Math.min(entries, heights.length, colors.length / 3)
  if (n < 1 || pw < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  let loudest = 0
  for (let x = 0; x < pw; x++) {
    const first = Math.floor((x * entries) / pw)
    if (first >= n) {
      amp[x] = NaN
      color[x] = -1
      continue
    }
    const end = Math.min(n, Math.max(first + 1, Math.floor(((x + 1) * entries) / pw)))
    let heightSum = 0
    let red = 0
    let green = 0
    let blue = 0
    for (let entry = first; entry < end; entry++) {
      const height = heights[entry]
      heightSum += height
      red += colors[entry * 3] * height
      green += colors[entry * 3 + 1] * height
      blue += colors[entry * 3 + 2] * height
    }
    const meanHeight = heightSum / (end - first)
    amp[x] = meanHeight
    color[x] = heightSum > 0 ? packRgb(red / heightSum, green / heightSum, blue / heightSum) : packColor(colors, first)
    loudest = Math.max(loudest, meanHeight)
  }
  const reach = ph * (1 - OVERVIEW_TOP_PAD)
  const fullScale = loudest || 1
  for (let x = 0; x < pw; x++) amp[x] = heightInPixels((amp[x] / fullScale) ** OVERVIEW_DYNAMICS_EXPONENT, reach)
  paintColumns(ctx, pw, ph, 'bottom', (x) => cssColor(color[x]))
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
 * `stride` reads every so-many entries as one; with `fall`, each level eases down after a peak.
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
  fall: BandFall | null = null
): void {
  const fallen = fall ? bandFall(bands, stride, fall) : null
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
