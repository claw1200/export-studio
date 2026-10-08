/** Drawing rekordbox-style analysis data: an RGB waveform, and 3-band waveforms. */
import { BANDS_LOW_TO_HIGH } from './colors'
import { columns, fitColumns, logicalCount, paintColumns, pixelGrid, readAmp, readRgbColumn } from './columns'
import { bandFall } from './fall'

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
  const reach = anchor === 'center' ? ph / 2 : ph
  for (let x = 0; x < pw; x++) {
    const start = (first + x) * per
    const col = readRgbColumn(heights, colors, n, start, start + per)
    if (!col) {
      amp[x] = NaN
      color[x] = -1
      continue
    }
    amp[x] = Math.max(1, Math.round(col.amp * reach))
    color[x] = col.color
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
  const prepared = fallBlocks >= 1 ? bandFall(bands, stride, fallBlocks) : null
  const data = prepared ?? bands
  const drawStride = prepared ? 1 : stride
  const src = data.length / 3
  const n = logicalCount(src, drawStride)
  const grid = pixelGrid(from, to, pw)
  if (!grid || n < 1) return
  fitColumns(pw)
  const { amp, color } = columns
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph
  const bandAt = (band: number, i: number): number => {
    if (drawStride <= 1) return data[i * 3 + band] / 255
    const a = Math.min(src, Math.floor(i * drawStride))
    const b = Math.min(src, Math.max(a + 1, Math.ceil((i + 1) * drawStride)))
    let peak = 0
    for (let j = a; j < b; j++) {
      const value = data[j * 3 + band]
      if (value > peak) peak = value
    }
    return peak / 255
  }
  for (const layer of BANDS_LOW_TO_HIGH) {
    for (let x = 0; x < pw; x++) {
      const start = (first + x) * per
      const level = readAmp((i) => bandAt(layer.band, i), n, start, start + per, prepared != null)
      amp[x] = Number.isNaN(level) ? NaN : level * reach
      color[x] = Number.isNaN(level) ? -1 : 1
    }
    paintColumns(ctx, pw, ph, anchor, () => layer.color, true)
  }
}

/** The whole track's bands stacked from the bottom, averaged per pixel, as the overview shows them. */
export function drawStackedBands(ctx: CanvasRenderingContext2D, bands: Uint8Array, pw: number, ph: number): void {
  const length = bands.length / 3
  const level = [new Float32Array(pw), new Float32Array(pw), new Float32Array(pw)]
  const floor0 = new Float32Array(pw)
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
  const scale = (ph - 1) / peak
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
