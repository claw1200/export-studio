import type { Beatgrid, Cue, TrackDetail } from './types'
import { HOT_CUE_DEFAULT, LOOP_DEFAULT, MEMORY_CUE_DEFAULT, TRACK_COLORS, hotCueLetter } from './format'

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

const BAND_COLORS = {
  low: [32, 92, 255],
  mid: [255, 166, 43],
  high: [245, 245, 255]
} as const

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

/**
 * Draws columns for entries [from, to) across `pw` device pixels.
 * `anchor` = 'bottom' for overview bars, 'center' for mirrored detail.
 */
function drawColumns(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  from: number,
  to: number,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center'
): void {
  const span = to - from
  if (span <= 0) return
  const per = span / pw
  const useBands = mode === '3band' && wf.bands
  const half = anchor === 'center'
  const full = half ? ph / 2 : ph
  const mid = ph / 2

  const bar = (x: number, height: number, color: string): void => {
    if (height <= 0) return
    const hh = Math.max(1, height * full)
    ctx.fillStyle = color
    if (half) ctx.fillRect(x, mid - hh, 1, hh * 2)
    else ctx.fillRect(x, ph - hh, 1, hh)
  }

  for (let x = 0; x < pw; x++) {
    const i0 = Math.floor(from + x * per)
    const i1 = Math.max(i0 + 1, Math.floor(from + (x + 1) * per))
    if (i1 <= 0 || i0 >= wf.length) continue
    const a = Math.max(0, i0)
    const b = Math.min(wf.length, i1)

    if (useBands) {
      const bands = wf.bands!
      let lo = 0
      let mi = 0
      let hi = 0
      for (let i = a; i < b; i++) {
        const o = i * 3
        if (bands[o] > lo) lo = bands[o]
        if (bands[o + 1] > mi) mi = bands[o + 1]
        if (bands[o + 2] > hi) hi = bands[o + 2]
      }
      const [lr, lg, lb] = BAND_COLORS.low
      const [mr, mg, mb] = BAND_COLORS.mid
      const [hr, hg, hb] = BAND_COLORS.high
      bar(x, lo / 255, `rgb(${lr},${lg},${lb})`)
      bar(x, (mi / 255) * 0.85, `rgb(${mr},${mg},${mb})`)
      bar(x, (hi / 255) * 0.6, `rgb(${hr},${hg},${hb})`)
    } else if (wf.rgbHeights && wf.rgbColors) {
      const hs = wf.rgbHeights
      const cs = wf.rgbColors
      let peak = 0
      let pi = a
      for (let i = a; i < b; i++) {
        if (hs[i] >= peak) {
          peak = hs[i]
          pi = i
        }
      }
      const o = pi * 3
      bar(x, peak / 255, `rgb(${cs[o]},${cs[o + 1]},${cs[o + 2]})`)
    }
  }
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
  else drawColumns(ctx, wf, mode, 0, wf.length, pw, ph, 'bottom')
}

/**
 * rekordbox-style 3-band overview: column averages drawn from the baseline with
 * highs at the back, mids over them and lows in front, sharing one vertical scale.
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
  const layers: [Float32Array, string][] = [
    [avg[2], 'rgb(244,236,220)'],
    [avg[1], 'rgb(232,170,80)'],
    [avg[0], 'rgb(0,92,224)']
  ]
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
  dpr: number
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
  drawColumns(ctx, wf, mode, from, to, pw, waveH, 'center')
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

  // cue markers
  ctx.font = `600 ${10 * dpr}px ui-sans-serif, system-ui, sans-serif`
  ctx.textBaseline = 'middle'
  for (const cue of view.cues) {
    const x = Math.round(toX(cue.start / 1000))
    if (x < -40 * dpr || x > pw + 40 * dpr) continue
    const color = cueColor(cue)
    ctx.fillStyle = color
    ctx.fillRect(x, 0, Math.max(1, dpr), ph)
    if (cue.kind === 'hot') {
      const w = 16 * dpr
      const h = 15 * dpr
      ctx.beginPath()
      ctx.roundRect(x, 0, w, h, [0, 3 * dpr, 3 * dpr, 0])
      ctx.fill()
      ctx.fillStyle = '#0b0c0f'
      ctx.fillText(hotCueLetter(cue.slot), x + 4.5 * dpr, h / 2 + 0.5 * dpr)
    } else {
      const s = 6 * dpr
      ctx.beginPath()
      ctx.moveTo(x - s, ph)
      ctx.lineTo(x + s, ph)
      ctx.lineTo(x, ph - s * 1.2)
      ctx.closePath()
      ctx.fill()
    }
  }

  // playhead
  const px = Math.round(pw / 2)
  ctx.fillStyle = 'rgba(0,0,0,0.5)'
  ctx.fillRect(px - dpr, 0, 3 * dpr, ph)
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(px, 0, Math.max(1, dpr), ph)
}
