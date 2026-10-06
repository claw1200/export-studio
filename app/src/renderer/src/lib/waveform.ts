import type { WaveSamples } from './peaks'
import type { Beatgrid, Cue, TrackDetail } from './types'
import { firstIndexAtOrAfter } from './beat'
import {
  HOT_CUE_DEFAULT,
  hotCueLetter,
  hotCuePaletteColor,
  LOOP_DEFAULT,
  MEMORY_CUE_DEFAULT,
  TRACK_COLORS
} from './format'

export type WaveformMode = 'rgb' | '3band'

export interface Waveform {
  rate: number
  length: number
  rgbHeights?: Uint8Array
  rgbColors?: Uint8Array
  bands?: Uint8Array
  previewBands?: Uint8Array
}

function b64(data: string): Uint8Array {
  const bin = atob(data)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

export function decodeWaveform(detail: TrackDetail): Waveform | null {
  const { rate, rgb, threeBand, threeBandPreview, mono } = detail.waveform
  const wf: Waveform = { rate, length: 0 }
  if (rgb) {
    wf.rgbHeights = b64(rgb.heights)
    wf.rgbColors = b64(rgb.colors)
    wf.length = rgb.length
  } else if (mono) {
    wf.rgbHeights = b64(mono.heights)
    const white = b64(mono.whiteness)
    const colors = new Uint8Array(white.length * 3)
    for (let i = 0; i < white.length; i++) {
      const w = white[i] / 255
      colors[i * 3] = 40 + 180 * w
      colors[i * 3 + 1] = 110 + 130 * w
      colors[i * 3 + 2] = 255
    }
    wf.rgbColors = colors
    wf.length = mono.length
  }
  if (threeBand) {
    wf.bands = b64(threeBand.bands)
    wf.length = Math.max(wf.length, threeBand.length)
  }
  if (threeBandPreview) wf.previewBands = b64(threeBandPreview.bands)
  return wf.length ? wf : null
}

export function cueColor(cue: Cue): string {
  if (cue.kind === 'hot') {
    return hotCuePaletteColor(cue.colorIndex) ?? cue.rgb ?? (cue.end != null ? LOOP_DEFAULT : HOT_CUE_DEFAULT)
  }
  if (cue.color != null && TRACK_COLORS[cue.color]) return TRACK_COLORS[cue.color].hex
  return MEMORY_CUE_DEFAULT
}

const LOW = 0
const MID = 1
const HIGH = 2
const BANDS_LOW_TO_HIGH: { band: number; color: string }[] = [
  { band: LOW, color: 'rgb(0,92,224)' },
  { band: MID, color: 'rgb(232,170,80)' },
  { band: HIGH, color: 'rgb(244,236,220)' }
]

let colAmp = new Float32Array(0)
let colColor = new Int32Array(0)

function fitColumns(n: number): void {
  if (colAmp.length >= n) return
  const size = Math.max(n, colAmp.length * 2)
  colAmp = new Float32Array(size)
  colColor = new Int32Array(size)
}

function packColor(colors: Uint8Array, index: number): number {
  const o = index * 3
  return (colors[o] << 16) | (colors[o + 1] << 8) | colors[o + 2]
}

function pixelGrid(from: number, to: number, pw: number): { per: number; first: number } | null {
  const span = to - from
  if (!(span > 0) || pw < 1) return null
  const per = span / pw
  if (!(per > 0)) return null
  return { per, first: Math.floor(from / per) }
}

function holdUntilPeak(current: number, next: number, fraction: number): number {
  if (next > current) return current
  return current + (next - current) * fraction
}

type ColumnSpan =
  { kind: 'between'; index: number; next: number; fraction: number } | { kind: 'peak'; from: number; to: number }

function columnSpan(count: number, start: number, end: number): ColumnSpan | null {
  if (!(end > start) || end <= 0 || start >= count) return null
  if (end - start <= 1) {
    const at = Math.min(count - 1, Math.max(0, (start + end) / 2))
    const index = at | 0
    return { kind: 'between', index, next: Math.min(count - 1, index + 1), fraction: at - index }
  }
  return { kind: 'peak', from: Math.max(0, Math.floor(start)), to: Math.min(count, Math.ceil(end)) }
}

function readAmp(get: (i: number) => number, count: number, start: number, end: number): number {
  const span = columnSpan(count, start, end)
  if (!span) return NaN
  if (span.kind === 'between') return holdUntilPeak(get(span.index), get(span.next), span.fraction)
  let peak = 0
  for (let i = span.from; i < span.to; i++) {
    const value = get(i)
    if (value > peak) peak = value
  }
  return peak
}

function readRgbColumn(
  heights: Uint8Array,
  colors: Uint8Array,
  count: number,
  start: number,
  end: number
): { amp: number; color: number } | null {
  const span = columnSpan(count, start, end)
  if (!span) return null
  if (span.kind === 'between') {
    return {
      amp: holdUntilPeak(heights[span.index], heights[span.next], span.fraction) / 255,
      color: packColor(colors, span.index)
    }
  }
  let peak = -1
  let peakIndex = span.from
  for (let i = span.from; i < span.to; i++) {
    if (heights[i] > peak) {
      peak = heights[i]
      peakIndex = i
    }
  }
  return { amp: Math.max(0, peak) / 255, color: packColor(colors, peakIndex) }
}

function paintColumns(
  ctx: CanvasRenderingContext2D,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center',
  colorAt: (x: number) => string
): void {
  const mid = ph / 2
  let x = 0
  while (x < pw) {
    if (Number.isNaN(colAmp[x]) || colColor[x] < 0) {
      x++
      continue
    }
    const color = colorAt(x)
    let end = x + 1
    while (end < pw && colColor[end] === colColor[x] && !Number.isNaN(colAmp[end])) end++
    ctx.fillStyle = color
    const x0 = x
    const right = end < pw ? end + 0.35 : end
    if (anchor === 'center') {
      ctx.beginPath()
      ctx.moveTo(x0, mid - colAmp[x0])
      for (let i = x0; i < end; i++) ctx.lineTo(i + 0.5, mid - colAmp[i])
      ctx.lineTo(right, mid - colAmp[end - 1])
      ctx.lineTo(right, mid + colAmp[end - 1])
      for (let i = end - 1; i >= x0; i--) ctx.lineTo(i + 0.5, mid + colAmp[i])
      ctx.closePath()
    } else {
      ctx.beginPath()
      ctx.moveTo(x0, ph)
      ctx.lineTo(x0, ph - colAmp[x0])
      for (let i = x0; i < end; i++) ctx.lineTo(i + 0.5, ph - colAmp[i])
      ctx.lineTo(right, ph - colAmp[end - 1])
      ctx.lineTo(right, ph)
      ctx.closePath()
    }
    ctx.fill()
    x = end
  }
}

function drawAnalysisRgb(
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
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph
  for (let x = 0; x < pw; x++) {
    const start = (first + x) * per
    const col = readRgbColumn(heights, colors, n, start, start + per)
    if (!col) {
      colAmp[x] = NaN
      colColor[x] = -1
      continue
    }
    colAmp[x] = Math.max(1, Math.round(col.amp * reach))
    colColor[x] = col.color
  }
  paintColumns(ctx, pw, ph, anchor, (x) => {
    const c = colColor[x]
    return `rgb(${(c >> 16) & 255},${(c >> 8) & 255},${c & 255})`
  })
}

function drawAnalysisBands(
  ctx: CanvasRenderingContext2D,
  bands: Uint8Array,
  from: number,
  to: number,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center'
): void {
  const n = bands.length / 3
  const grid = pixelGrid(from, to, pw)
  if (!grid || n < 1) return
  fitColumns(pw)
  const { per, first } = grid
  const reach = anchor === 'center' ? ph / 2 : ph
  for (const layer of BANDS_LOW_TO_HIGH) {
    for (let x = 0; x < pw; x++) {
      const start = (first + x) * per
      const amp = readAmp((i) => bands[i * 3 + layer.band] / 255, n, start, start + per)
      colAmp[x] = Number.isNaN(amp) ? NaN : Math.max(1, Math.round(amp * reach))
      colColor[x] = Number.isNaN(amp) ? -1 : 1
    }
    paintColumns(ctx, pw, ph, anchor, () => layer.color)
  }
}

export function setupCanvas(canvas: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D {
  const dpr = window.devicePixelRatio || 1
  const pw = Math.max(1, Math.round(w * dpr))
  const ph = Math.max(1, Math.round(h * dpr))
  if (canvas.width !== pw || canvas.height !== ph) {
    canvas.width = pw
    canvas.height = ph
  }
  const ctx = canvas.getContext('2d')!
  ctx.setTransform(1, 0, 0, 1, 0, 0)
  return ctx
}

export function drawOverview(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  pw: number,
  ph: number
): void {
  ctx.clearRect(0, 0, pw, ph)
  const bands = mode === '3band' ? (wf.previewBands ?? wf.bands) : null
  if (bands) drawStackedBands(ctx, bands, pw, ph)
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, 0, wf.length, pw, ph, 'bottom')
}

function drawStackedBands(ctx: CanvasRenderingContext2D, bands: Uint8Array, pw: number, ph: number): void {
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

export interface DetailView {
  position: number
  zoom: number
  cues: Cue[]
  beatgrid: Beatgrid | null
  cuePoint: number | null
}

export const CUE_POINT_COLOR = '#ff8a1f'

let envUpper = new Float32Array(0)
let envLower = new Float32Array(0)

function drawSampleWave(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  samples: WaveSamples,
  from: number,
  to: number,
  pw: number,
  ph: number
): void {
  const span = to - from
  const data = samples.data
  const len = data.length
  if (span <= 0 || pw < 1 || ph < 1 || len < 2) return
  if (envUpper.length < pw) {
    envUpper = new Float32Array(pw)
    envLower = new Float32Array(pw)
  }
  const upper = envUpper
  const lower = envLower
  const sample0 = (from / wf.rate) * samples.rate
  const per = (span / wf.rate) * (samples.rate / pw)
  if (!(per > 0)) return
  const first = Math.floor(sample0 / per)
  const mid = ph / 2
  const scale = (ph * 0.46) / samples.peak

  const at = (t: number): number => {
    if (t <= 0) return data[0]
    if (t >= len - 1) return data[len - 1]
    const i = t | 0
    return data[i] + (data[i + 1] - data[i]) * (t - i)
  }

  for (let x = 0; x < pw; x++) {
    const a = (first + x) * per
    const b = a + per
    if (b <= 0 || a >= len - 1) {
      upper[x] = NaN
      continue
    }
    const left = a < 0 ? 0 : a
    const right = b > len - 1 ? len - 1 : b
    let lo = at(left)
    let hi = lo
    const endSample = at(right)
    if (endSample < lo) lo = endSample
    else if (endSample > hi) hi = endSample
    for (let i = Math.ceil(left); i <= Math.floor(right); i++) {
      const s = data[i]
      if (s < lo) lo = s
      else if (s > hi) hi = s
    }
    let top = Math.round(mid - hi * scale)
    let bot = Math.round(mid - lo * scale)
    if (bot - top < 1) {
      const y = Math.round(mid - ((lo + hi) / 2) * scale)
      top = y
      bot = y + 1
    }
    upper[x] = top
    lower[x] = bot
  }

  const colors = wf.rgbColors
  const colorAt = (x: number): number => {
    if (!colors || colors.length < 3) return -1
    const seconds = ((first + x + 0.5) * per) / samples.rate
    const entry = Math.floor(seconds * wf.rate)
    return Math.min(colors.length / 3 - 1, Math.max(0, entry))
  }

  let x = 0
  while (x < pw) {
    if (Number.isNaN(upper[x])) {
      x++
      continue
    }
    const colorIndex = colorAt(x)
    let end = x + 1
    while (end < pw && !Number.isNaN(upper[end]) && colorAt(end) === colorIndex) end++
    if (colors && colorIndex >= 0) {
      const o = colorIndex * 3
      ctx.fillStyle = `rgb(${colors[o]},${colors[o + 1]},${colors[o + 2]})`
    } else ctx.fillStyle = 'rgb(90,150,255)'
    for (let i = x; i < end; i++) ctx.fillRect(i, upper[i], 1, lower[i] - upper[i])
    x = end
  }
}

export function visibleRange(wf: Waveform, view: DetailView, cssWidth: number): [number, number] {
  const center = view.position * wf.rate
  const halfEntries = cssWidth / 2 / view.zoom
  return [center - halfEntries, center + halfEntries]
}

interface Frame {
  ctx: CanvasRenderingContext2D
  wf: Waveform
  view: DetailView
  pw: number
  ph: number
  dpr: number
  from: number
  to: number
  toX: (seconds: number) => number
}

function paintLoops(frame: Frame): void {
  const { ctx, view, pw, ph, toX } = frame
  for (const cue of view.cues) {
    if (cue.end == null) continue
    const x0 = toX(cue.start / 1000)
    const x1 = toX(cue.end / 1000)
    if (x1 < 0 || x0 > pw) continue
    ctx.fillStyle = cueColor(cue) + '33'
    ctx.fillRect(x0, 0, x1 - x0, ph)
  }
}

function paintWave(frame: Frame, mode: WaveformMode, samples?: WaveSamples | null): void {
  const { ctx, wf, from, to, pw, ph, dpr } = frame
  const waveH = ph - 18 * dpr
  ctx.save()
  ctx.translate(0, 9 * dpr)
  if (mode === 'rgb' && samples) drawSampleWave(ctx, wf, samples, from, to, pw, waveH)
  else if (mode === '3band' && wf.bands) drawAnalysisBands(ctx, wf.bands, from, to, pw, waveH, 'center')
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, from, to, pw, waveH, 'center')
  ctx.restore()
}

function paintBeatGrid(frame: Frame): void {
  const { ctx, wf, view, ph, dpr, from, to, toX } = frame
  const grid = view.beatgrid
  if (!grid) return
  const startMs = (from / wf.rate) * 1000
  const endMs = (to / wf.rate) * 1000
  const cueXs = view.cues.map((cue) => Math.round(toX(cue.start / 1000)))
  const coveredByCue = (x: number): boolean => cueXs.some((cueX) => Math.abs(cueX - x) <= 2 * dpr)
  for (let i = firstIndexAtOrAfter(grid.times, startMs); i < grid.times.length && grid.times[i] <= endMs; i++) {
    const x = Math.round(toX(grid.times[i] / 1000))
    if (coveredByCue(x)) continue
    const downbeat = grid.beats[i] === 1
    const tick = (downbeat ? 7 : 4) * dpr
    ctx.fillStyle = downbeat ? 'rgba(255,72,88,0.9)' : 'rgba(255,255,255,0.28)'
    ctx.fillRect(x, 0, Math.max(1, dpr), tick)
    ctx.fillRect(x, ph - tick, Math.max(1, dpr), tick)
    if (downbeat) {
      ctx.fillStyle = 'rgba(255,255,255,0.10)'
      ctx.fillRect(x, 0, Math.max(1, dpr), ph)
    }
  }
}

function memoryCueBeforeHotCue(a: Cue, b: Cue): number {
  return Number(a.kind === 'hot') - Number(b.kind === 'hot')
}

function paintCues(frame: Frame): void {
  const { ctx, view, pw, ph, dpr, toX } = frame
  const diamond = 6 * dpr
  const stem = Math.max(1, dpr)
  const hotStem = Math.max(2, Math.round(1.5 * dpr))
  const tipHeight = Math.round(diamond * 1.2)
  const badge = Math.round(14 * dpr)
  ctx.font = `700 ${Math.round(9.5 * dpr)}px "Inter Variable", system-ui, sans-serif`
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  for (const cue of [...view.cues].sort(memoryCueBeforeHotCue)) {
    const x = Math.round(toX(cue.start / 1000))
    if (x < -40 * dpr || x > pw + 40 * dpr) continue
    ctx.fillStyle = cueColor(cue)
    if (cue.kind === 'hot') {
      ctx.fillRect(x, 0, hotStem, ph)
      ctx.fillRect(x, ph - badge, badge, badge)
      ctx.fillStyle = 'rgba(0,0,0,0.85)'
      ctx.fillText(hotCueLetter(cue.slot), x + badge / 2, ph - badge / 2 + 0.5 * dpr)
      continue
    }
    const tip = x + stem / 2
    ctx.fillRect(x, 0, stem, ph)
    ctx.beginPath()
    ctx.moveTo(tip - diamond, 0)
    ctx.lineTo(tip + diamond, 0)
    ctx.lineTo(tip, tipHeight)
    ctx.closePath()
    ctx.fill()
  }
}

function paintCuePoint(frame: Frame): void {
  const { ctx, view, pw, ph, dpr, toX } = frame
  if (view.cuePoint == null) return
  const x = Math.round(toX(view.cuePoint))
  const size = 7 * dpr
  if (x < -size || x > pw + size) return
  const stem = Math.max(1, dpr)
  const tip = x + stem / 2
  ctx.fillStyle = CUE_POINT_COLOR
  ctx.fillRect(x, 0, stem, ph)
  ctx.beginPath()
  ctx.moveTo(tip - size, 0)
  ctx.lineTo(tip + size, 0)
  ctx.lineTo(tip, Math.round(size * 1.2))
  ctx.closePath()
  ctx.fill()
}

function paintPlayhead(frame: Frame): void {
  const { ctx, pw, ph, dpr } = frame
  const x = Math.round(pw / 2)
  ctx.fillStyle = 'rgba(0,0,0,0.5)'
  ctx.fillRect(x - dpr, 0, 3 * dpr, ph)
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(x, 0, Math.max(1, dpr), ph)
}

export function drawDetail(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  view: DetailView,
  pw: number,
  ph: number,
  dpr: number,
  samples?: WaveSamples | null
): void {
  ctx.clearRect(0, 0, pw, ph)
  const [from, to] = visibleRange(wf, view, pw / dpr)
  const frame: Frame = {
    ctx,
    wf,
    view,
    pw,
    ph,
    dpr,
    from,
    to,
    toX: (seconds) => ((seconds * wf.rate - from) / (to - from)) * pw
  }
  paintLoops(frame)
  paintWave(frame, mode, samples)
  paintBeatGrid(frame)
  paintCues(frame)
  paintCuePoint(frame)
  paintPlayhead(frame)
}
