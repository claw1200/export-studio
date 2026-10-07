// Numeric waveform work. Runs on the main thread for small jobs and in compute.worker.ts for the rest.

export const ENVELOPE_RATE = 16_000
export const MAX_ENVELOPE_SAMPLES = 6_000_000
export const BAND_RATE = 8_000
export const MAX_BAND_FRAMES = 4_000_000
const LOW_HZ = 200
const HIGH_HZ = 2_000
/** Samples of filter history a chunk replays so its peaks match a single full pass. */
const BAND_WARMUP_SEC = 0.08

interface Biquad {
  b0: number
  b1: number
  b2: number
  a1: number
  a2: number
  x1: number
  x2: number
  y1: number
  y2: number
}

export interface WaveRates {
  bandStep: number
  bandFrames: number
  bandRate: number
  envStep: number
  envFrames: number
  envRate: number
}

export interface ScanSpec {
  origin: number
  to: number
  monoLength: number
  sampleRate: number
  bandStep: number
  bandK0: number
  bandFrames: number
  bandFilterFrom: number
  envStep: number
  envK0: number
  envFrames: number
}

export interface ScanOut {
  bands: Uint8Array
  bandMax: number
  env: Int16Array
  envPeak: number
}

export type ComputeRequest =
  | ({
      op: 'scan'
      samples: Float32Array
    } & ScanSpec)
  | { op: 'fall'; column: Uint8Array; left: number; count: number; reach: number }

export type ComputeResponse =
  | ({ op: 'scan' } & ScanOut)
  | { op: 'fall'; out: Uint8Array }
  | { op: 'error'; message: string }

export function mixMono(left: Float32Array, right: Float32Array | null): Float32Array {
  const mono = new Float32Array(left.length)
  if (!right) {
    mono.set(left)
    return mono
  }
  for (let i = 0; i < left.length; i++) mono[i] = (left[i] + right[i]) * 0.5
  return mono
}

export function waveRates(length: number, sampleRate: number): WaveRates {
  const duration = length / sampleRate
  const bandRate = Math.min(BAND_RATE, sampleRate, MAX_BAND_FRAMES / duration)
  const envRate = Math.min(ENVELOPE_RATE, MAX_ENVELOPE_SAMPLES / duration)
  const bandStep = sampleRate / bandRate
  const envStep = sampleRate / envRate
  return {
    bandStep,
    bandFrames: Math.floor(length / bandStep),
    bandRate: 0,
    envStep,
    envFrames: Math.floor(length / envStep),
    envRate: 0
  }
}

export function reportedRate(frames: number, length: number, sampleRate: number): number {
  return length > 0 ? (frames * sampleRate) / length : 0
}

export function planWave(
  length: number,
  sampleRate: number,
  parts: number,
  wantBands: boolean,
  wantEnv: boolean
): ScanSpec[] {
  const rates = waveRates(length, sampleRate)
  const bandFrames = wantBands ? rates.bandFrames : 0
  const envFrames = wantEnv ? rates.envFrames : 0
  const slices = Math.max(1, Math.min(parts, Math.max(bandFrames, envFrames, 1)))
  const span = Math.ceil(length / slices)
  const warmup = Math.ceil(sampleRate * BAND_WARMUP_SEC)
  const specs: ScanSpec[] = []
  for (let part = 0; part < slices; part++) {
    const sample0 = part * span
    const sample1 = Math.min(length, sample0 + span)
    const band = framesIn(rates.bandStep, bandFrames, sample0, sample1)
    const env = framesIn(rates.envStep, envFrames, sample0, sample1)
    if (band.frames === 0 && env.frames === 0) continue
    const bandStart = band.frames > 0 ? Math.floor(band.k0 * rates.bandStep) : sample0
    const bandFilterFrom = band.frames > 0 ? Math.max(0, bandStart - (band.k0 === 0 ? 0 : warmup)) : bandStart
    const bandEnd = band.frames > 0 ? frameEnd(band.k0 + band.frames - 1, rates.bandStep, length) : sample0
    const envStart = env.frames > 0 ? Math.floor(env.k0 * rates.envStep) : bandFilterFrom
    const envEnd = env.frames > 0 ? frameEnd(env.k0 + env.frames - 1, rates.envStep, length) : bandEnd
    const origin = Math.min(bandFilterFrom, envStart)
    const to = Math.max(bandEnd, envEnd)
    specs.push({
      origin,
      to,
      monoLength: length,
      sampleRate,
      bandStep: rates.bandStep,
      bandK0: band.k0,
      bandFrames: band.frames,
      bandFilterFrom,
      envStep: rates.envStep,
      envK0: env.k0,
      envFrames: env.frames
    })
  }
  return specs
}

function framesIn(step: number, frameCount: number, sample0: number, sample1: number): { k0: number; frames: number } {
  if (frameCount <= 0 || !(sample1 > sample0)) return { k0: 0, frames: 0 }
  let k0 = Math.min(frameCount, Math.max(0, Math.ceil(sample0 / step)))
  while (k0 < frameCount && Math.floor(k0 * step) < sample0) k0++
  let k1 = k0
  while (k1 < frameCount && Math.floor(k1 * step) < sample1) k1++
  return { k0, frames: k1 - k0 }
}

function frameEnd(frame: number, step: number, length: number): number {
  const start = Math.floor(frame * step)
  return Math.min(length, Math.max(start + 1, Math.floor((frame + 1) * step)))
}

function biquad(kind: 'low' | 'high', sampleRate: number, freq: number): Biquad {
  const w0 = (2 * Math.PI * freq) / sampleRate
  const cos = Math.cos(w0)
  const alpha = Math.sin(w0) / (2 * Math.SQRT1_2)
  const a0 = 1 + alpha
  const b0 = kind === 'low' ? (1 - cos) / 2 : (1 + cos) / 2
  const b1 = kind === 'low' ? 1 - cos : -(1 + cos)
  return {
    b0: b0 / a0,
    b1: b1 / a0,
    b2: b0 / a0,
    a1: (-2 * cos) / a0,
    a2: (1 - alpha) / a0,
    x1: 0,
    x2: 0,
    y1: 0,
    y2: 0
  }
}

function filter(f: Biquad, x: number): number {
  const y = f.b0 * x + f.b1 * f.x1 + f.b2 * f.x2 - f.a1 * f.y1 - f.a2 * f.y2
  f.x2 = f.x1
  f.x1 = x
  f.y2 = f.y1
  f.y1 = y
  return y
}

function quantBand(peak: number): number {
  return peak <= 0 ? 0 : Math.min(255, Math.round(peak * 255))
}

export function scanPart(spec: ScanSpec, samples: Float32Array): ScanOut {
  const at = (index: number): number => samples[index - spec.origin]
  const bands = new Uint8Array(spec.bandFrames * 3)
  let bandMax = 0
  if (spec.bandFrames > 0) {
    const low = biquad('low', spec.sampleRate, LOW_HZ)
    const midHigh = biquad('high', spec.sampleRate, LOW_HZ)
    const midLow = biquad('low', spec.sampleRate, HIGH_HZ)
    const high = biquad('high', spec.sampleRate, HIGH_HZ)
    let index = spec.bandFilterFrom
    const recordFrom = Math.floor(spec.bandK0 * spec.bandStep)
    for (; index < recordFrom; index++) {
      const v = at(index)
      filter(low, v)
      const mid = filter(midHigh, v)
      filter(midLow, mid)
      filter(high, v)
    }
    for (let k = 0; k < spec.bandFrames; k++) {
      const end = frameEnd(spec.bandK0 + k, spec.bandStep, spec.monoLength)
      let lowPeak = 0
      let midPeak = 0
      let highPeak = 0
      for (; index < end; index++) {
        const v = at(index)
        const lo = Math.abs(filter(low, v))
        const mid = Math.abs(filter(midLow, filter(midHigh, v)))
        const hi = Math.abs(filter(high, v))
        if (lo > lowPeak) lowPeak = lo
        if (mid > midPeak) midPeak = mid
        if (hi > highPeak) highPeak = hi
      }
      const o = k * 3
      const lq = quantBand(lowPeak)
      const mq = quantBand(midPeak)
      const hq = quantBand(highPeak)
      bands[o] = lq
      bands[o + 1] = mq
      bands[o + 2] = hq
      if (lq > bandMax) bandMax = lq
      if (mq > bandMax) bandMax = mq
      if (hq > bandMax) bandMax = hq
    }
  }

  const env = new Int16Array(spec.envFrames)
  let envPeak = 0
  for (let k = 0; k < spec.envFrames; k++) {
    const frame = spec.envK0 + k
    const start = Math.floor(frame * spec.envStep)
    const end = frameEnd(frame, spec.envStep, spec.monoLength)
    let s = 0
    for (let j = start; j < end; j++) {
      const v = at(j)
      if (Math.abs(v) > Math.abs(s)) s = v
    }
    const q = Math.max(-32767, Math.min(32767, Math.round(s * 32767)))
    env[k] = q
    const a = q < 0 ? -q : q
    if (a > envPeak) envPeak = a
  }
  return { bands, bandMax, env, envPeak }
}

export function normalizeBands(data: Uint8Array, max: number): void {
  if (!(max > 8 && max < 255)) return
  const scale = 255 / max
  for (let i = 0; i < data.length; i++) data[i] = Math.min(255, Math.round(data[i] * scale))
}

/** Upper envelope of falls that land `reach` samples ahead. Rises stay flat. */
export function fallSlice(column: Uint8Array, left: number, count: number, reach: number): Uint8Array {
  const out = new Uint8Array(count)
  const end = left + count
  for (let i = left; i < end; i++) {
    let best = column[i] * reach
    const maxK = i < reach ? i : reach - 1
    for (let k = 1; k <= maxK; k++) {
      const peak = column[i - k]
      const dest = column[i - k + reach]
      const scaled = dest >= peak ? peak * reach : peak * (reach - k) + dest * k
      if (scaled > best) best = scaled
    }
    const rounded = Math.round(best / reach)
    out[i - left] = rounded > 255 ? 255 : rounded
  }
  return out
}
