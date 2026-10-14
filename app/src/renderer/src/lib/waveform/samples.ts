/** Drawing the engine's own analysis of the decoded audio: a sample-accurate envelope. */
import type { WaveSamples } from '../deck/waves.svelte'
import type { Waveform } from './data'

let envUpper = new Float32Array(0)
let envLower = new Float32Array(0)

/** Entries read as one at `quality` (1–100): every entry at 100. */
export function waveStride(quality: number): number {
  return quality >= 100 ? 1 : 100 / Math.max(1, quality)
}

/**
 * Entries `from`–`to` of `wf` (its rate), drawn from `samples` in the analysis's colours.
 * `headroom` is the share of the height left clear above a full-scale peak.
 */
export function drawSampleWave(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  samples: WaveSamples,
  from: number,
  to: number,
  pw: number,
  ph: number,
  quality: number,
  headroom = 0.04
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
  const stride = waveStride(quality)
  const sample0 = (from / wf.rate) * samples.rate
  const per = (span / wf.rate) * (samples.rate / pw)
  if (!(per > 0)) return
  const first = Math.floor(sample0 / per)
  const mid = ph / 2
  const scale = (ph * (0.5 - headroom)) / samples.peak

  const at = (t: number): number => {
    if (t <= 0) return data[0]
    if (t >= len - 1) return data[len - 1]
    const i = t | 0
    return data[i] + (data[i + 1] - data[i]) * (t - i)
  }

  for (let x = 0; x < pw; x++) {
    let a = (first + x) * per
    let b = a + per
    if (stride > 1) {
      if (b - a <= stride) {
        const bucket = Math.floor((a + b) / 2 / stride)
        a = bucket * stride
        b = a + stride
      } else {
        a = Math.floor(a / stride) * stride
        b = Math.ceil(b / stride) * stride
      }
    }
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
