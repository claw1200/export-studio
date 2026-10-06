import type { WaveSamples } from './peaks'
import type { Beatgrid, Cue, TrackDetail } from './types'
import { HOT_CUE_DEFAULT, LOOP_DEFAULT, MEMORY_CUE_DEFAULT, TRACK_COLORS } from './format'

export type WaveformMode = 'rgb' | '3band'

export interface Waveform {
  rate: number
  length: number
  /** heights 0–255, one per entry */
  rgbHeights?: Uint8Array
  /** r,g,b triplets 0–255 */
  rgbColors?: Uint8Array
  /** low,mid,high triplets 0–255 */
  bands?: Uint8Array
  /** low,mid,high triplets from the fixed-width preview tag */
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
  if (cue.kind === 'hot') return cue.rgb ?? (cue.end != null ? LOOP_DEFAULT : HOT_CUE_DEFAULT)
  if (cue.color != null && TRACK_COLORS[cue.color]) return TRACK_COLORS[cue.color].hex
  return MEMORY_CUE_DEFAULT
}

/** Bass first, then mids, then highs on top. Same scale for every band. */
const BAND_LAYERS: { band: number; color: string }[] = [
  { band: 0, color: 'rgb(0,92,224)' },
  { band: 1, color: 'rgb(232,170,80)' },
  { band: 2, color: 'rgb(244,236,220)' }
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

/**
 * Pixel columns anchored at entry 0. `first` is an integer, so a moving playhead
 * scrolls whole pixels instead of resampling the wave every frame.
 */
function pixelGrid(from: number, to: number, pw: number): { per: number; first: number } | null {
  const span = to - from
  if (!(span > 0) || pw < 1) return null
  const per = span / pw
  if (!(per > 0)) return null
  return { per, first: Math.floor(from / per) }
}

/**
 * Join two analysis samples. A rise stays at the quieter sample until the louder
 * one, so a hit does not fade in ahead of itself. A fall interpolates.
 */
function joinAmp(current: number, next: number, fraction: number): number {
  if (next > current) return current
  return current + (next - current) * fraction
}

/**
 * Loudness for one pixel from the 150 Hz analysis.
 * A pixel narrower than one entry joins the stored samples.
 * A wider pixel keeps the peak so transients survive zooming out.
 */
function readAmp(get: (i: number) => number, n: number, start: number, end: number): number {
  if (!(end > start) || end <= 0 || start >= n) return NaN
  if (end - start <= 1) {
    const t = Math.min(n - 1, Math.max(0, (start + end) / 2))
    const i = t | 0
    const j = Math.min(n - 1, i + 1)
    return joinAmp(get(i), get(j), t - i)
  }
  const i0 = Math.max(0, Math.floor(start))
  const i1 = Math.min(n, Math.ceil(end))
  let peak = 0
  for (let i = i0; i < i1; i++) {
    const v = get(i)
    if (v > peak) peak = v
  }
  return peak
}

function readRgbColumn(
  heights: Uint8Array,
  colors: Uint8Array,
  n: number,
  start: number,
  end: number
): { amp: number; color: number } | null {
  if (!(end > start) || end <= 0 || start >= n) return null
  if (end - start <= 1) {
    const t = Math.min(n - 1, Math.max(0, (start + end) / 2))
    const i = t | 0
    const j = Math.min(n - 1, i + 1)
    return { amp: joinAmp(heights[i], heights[j], t - i) / 255, color: packColor(colors, i) }
  }
  const i0 = Math.max(0, Math.floor(start))
  const i1 = Math.min(n, Math.ceil(end))
  let peak = -1
  let pi = i0
  for (let i = i0; i < i1; i++) {
    if (heights[i] > peak) {
      peak = heights[i]
      pi = i
    }
  }
  return { amp: Math.max(0, peak) / 255, color: packColor(colors, pi) }
}

/** Filled shape. `anchor` center mirrors around the middle; bottom grows up from the baseline. */
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

/**
 * Color waveform from the stored analysis. Each entry is only a loudness and a color;
 * neighboring entries are joined so the outline follows the data instead of sitting flat.
 */
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

/** 3-band waveform from the stored analysis, layered and joined the same way. */
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
  for (const layer of BAND_LAYERS) {
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
  if (mode === '3band' && wf.previewBands) drawOverviewBands(ctx, wf.previewBands, pw, ph)
  else if (mode === '3band' && wf.bands) drawOverviewBands(ctx, wf.bands, pw, ph)
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, 0, wf.length, pw, ph, 'bottom')
}

/**
 * 3-band overview: column averages from the baseline, sharing one vertical scale.
 * Highs are painted first and bass last, so blue sits at the bottom, then mids, then highs.
 */
function drawOverviewBands(ctx: CanvasRenderingContext2D, bands: Uint8Array, pw: number, ph: number): void {
  const length = bands.length / 3
  const avg = [new Float32Array(pw), new Float32Array(pw), new Float32Array(pw)]
  let peak = 1
  for (let x = 0; x < pw; x++) {
    const a = Math.floor((x * length) / pw)
    const b = Math.max(a + 1, Math.floor(((x + 1) * length) / pw))
    for (let band = 0; band < 3; band++) {
      let sum = 0
      for (let i = a; i < b; i++) sum += bands[i * 3 + band]
      avg[band][x] = sum / (b - a)
      if (avg[band][x] > peak) peak = avg[band][x]
    }
  }
  const scale = (ph - 1) / peak
  const layers: [Float32Array, string][] = [...BAND_LAYERS]
    .reverse()
    .map((layer) => [avg[layer.band], layer.color])
  for (const [values, color] of layers) {
    ctx.fillStyle = color
    ctx.beginPath()
    ctx.moveTo(0, ph)
    for (let x = 0; x < pw; x++) ctx.lineTo(x + 0.5, ph - values[x] * scale)
    ctx.lineTo(pw, ph)
    ctx.closePath()
    ctx.fill()
  }
}

export interface DetailView {
  /** playhead position in seconds */
  position: number
  /** css pixels per waveform entry */
  zoom: number
  cues: Cue[]
  beatgrid: Beatgrid | null
}

let envUpper = new Float32Array(0)
let envLower = new Float32Array(0)

/**
 * Bipolar trace of the decoded audio, coloured by the RGB analysis.
 * Columns are locked to a grid anchored at sample 0 and drawn on whole pixels,
 * so playback scrolls the picture instead of resampling every frame.
 */
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
  const toX = (seconds: number): number => ((seconds * wf.rate - from) / (to - from)) * pw

  // loops behind everything
  for (const cue of view.cues) {
    if (cue.end == null) continue
    const x0 = toX(cue.start / 1000)
    const x1 = toX(cue.end / 1000)
    if (x1 < 0 || x0 > pw) continue
    ctx.fillStyle = cueColor(cue) + '33'
    ctx.fillRect(x0, 0, x1 - x0, ph)
  }

  const waveH = ph - 18 * dpr
  ctx.save()
  ctx.translate(0, 9 * dpr)
  if (mode === 'rgb' && samples) drawSampleWave(ctx, wf, samples, from, to, pw, waveH)
  else if (mode === '3band' && wf.bands) drawAnalysisBands(ctx, wf.bands, from, to, pw, waveH, 'center')
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, from, to, pw, waveH, 'center')
  ctx.restore()

  // beat grid
  const grid = view.beatgrid
  if (grid) {
    const t0 = (from / wf.rate) * 1000
    const t1 = (to / wf.rate) * 1000
    let lo = 0
    let hi = grid.times.length
    while (lo < hi) {
      const m = (lo + hi) >> 1
      if (grid.times[m] < t0) lo = m + 1
      else hi = m
    }
    for (let i = lo; i < grid.times.length && grid.times[i] <= t1; i++) {
      const x = Math.round(toX(grid.times[i] / 1000))
      const down = grid.beats[i] === 1
      ctx.fillStyle = down ? 'rgba(255,72,88,0.9)' : 'rgba(255,255,255,0.28)'
      ctx.fillRect(x, 0, Math.max(1, dpr), down ? 7 * dpr : 4 * dpr)
      ctx.fillRect(x, ph - (down ? 7 : 4) * dpr, Math.max(1, dpr), (down ? 7 : 4) * dpr)
      if (down) {
        ctx.fillStyle = 'rgba(255,255,255,0.10)'
        ctx.fillRect(x, 0, Math.max(1, dpr), ph)
      }
    }
  }

  // cue markers: memory cues hang from the top, hot cues sit on the bottom.
  // The stem is `mark` device pixels wide starting at `x`; the tip sits on its center
  // so it stays locked to the beat grid instead of riding the stem's left edge.
  const s = 6 * dpr
  const mark = Math.max(1, dpr)
  const tipH = Math.round(s * 1.2)
  for (const cue of view.cues) {
    const x = Math.round(toX(cue.start / 1000))
    if (x < -40 * dpr || x > pw + 40 * dpr) continue
    const tip = x + mark / 2
    ctx.fillStyle = cueColor(cue)
    ctx.fillRect(x, 0, mark, ph)
    ctx.beginPath()
    if (cue.kind === 'hot') {
      ctx.moveTo(tip - s, ph)
      ctx.lineTo(tip + s, ph)
      ctx.lineTo(tip, ph - tipH)
    } else {
      ctx.moveTo(tip - s, 0)
      ctx.lineTo(tip + s, 0)
      ctx.lineTo(tip, tipH)
    }
    ctx.closePath()
    ctx.fill()
  }

  // playhead
  const px = Math.round(pw / 2)
  ctx.fillStyle = 'rgba(0,0,0,0.5)'
  ctx.fillRect(px - dpr, 0, 3 * dpr, ph)
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(px, 0, Math.max(1, dpr), ph)
}
