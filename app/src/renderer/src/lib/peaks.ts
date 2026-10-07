import { Lru } from './lru'
import { mixMono, normalizeBands, planWave, reportedRate, scanPart, waveRates, type ScanOut } from './dsp'
import { poolSize, runJob } from './pool'

export interface WaveSamples {
  rate: number
  data: Int16Array
  peak: number
  /** Low, mid, high peak per bin, 0–255, normalized so the loudest bin is 255. */
  bands?: Uint8Array
  bandRate?: number
}

const CACHE_SIZE = 2

const cache = new Lru<string, WaveSamples>(CACHE_SIZE)
let context: AudioContext | null = null

export function cachedWaveSamples(url: string): WaveSamples | null {
  return cache.get(url) ?? null
}

export async function loadWaveSamples(
  url: string,
  signal: AbortSignal,
  bands = false
): Promise<WaveSamples | null> {
  const hit = cachedWaveSamples(url)
  if (hit && (!bands || hit.bands)) return hit
  try {
    const res = await fetch(url, { signal })
    if (!res.ok) return hit
    const buf = await res.arrayBuffer()
    if (signal.aborted) return null
    context ??= new AudioContext()
    const audio = await context.decodeAudioData(buf)
    if (signal.aborted) return null
    const left = audio.getChannelData(0)
    if (left.length < 2) return hit
    const right = audio.numberOfChannels > 1 ? audio.getChannelData(1) : null
    const mono = mixMono(left, right)
    if (signal.aborted) return null
    const built = await buildWave(mono, audio.sampleRate, !hit, bands && !hit?.bands, signal)
    if (!built || signal.aborted) return null
    const samples: WaveSamples = hit
      ? { ...hit }
      : { rate: built.envRate, data: built.env, peak: built.envPeak }
    if (built.bands) {
      samples.bands = built.bands
      samples.bandRate = built.bandRate
    }
    cache.set(url, samples)
    return samples
  } catch (err) {
    if (signal.aborted || (err as Error).name === 'AbortError') return null
    console.warn('[waveform] could not read audio', err)
    return null
  }
}

interface BuiltWave {
  env: Int16Array
  envRate: number
  envPeak: number
  bands?: Uint8Array
  bandRate?: number
}

async function buildWave(
  mono: Float32Array,
  sampleRate: number,
  wantEnv: boolean,
  wantBands: boolean,
  signal: AbortSignal
): Promise<BuiltWave | null> {
  const rates = waveRates(mono.length, sampleRate)
  if (wantEnv && rates.envFrames < 2) return null
  if (wantBands && rates.bandFrames < 2) wantBands = false
  if (!wantEnv && !wantBands) return { env: new Int16Array(), envRate: 0, envPeak: 1 }
  const specs = planWave(mono.length, sampleRate, poolSize(), wantBands, wantEnv)
  const parts = await Promise.all(
    specs.map(async (spec) => {
      if (signal.aborted) return null
      const samples = new Float32Array(mono.subarray(spec.origin, spec.to))
      try {
        const response = await runJob({ op: 'scan', samples, ...spec }, [samples.buffer])
        if (response.op !== 'scan') throw new Error('unexpected worker result')
        return response
      } catch (err) {
        if (signal.aborted) return null
        console.warn('[waveform] worker scan failed', err)
        return scanPart(spec, mono.subarray(spec.origin, spec.to))
      }
    })
  )
  if (signal.aborted || parts.some((part) => !part)) return null
  const scanned = parts as ScanOut[]
  const built: BuiltWave = { env: new Int16Array(), envRate: 0, envPeak: 1 }
  if (wantEnv) {
    let envPeak = 0
    let count = 0
    for (const part of scanned) {
      count += part.env.length
      if (part.envPeak > envPeak) envPeak = part.envPeak
    }
    const env = new Int16Array(count)
    let offset = 0
    for (const part of scanned) {
      env.set(part.env, offset)
      offset += part.env.length
    }
    built.env = env
    built.envPeak = Math.max(1, envPeak)
    built.envRate = reportedRate(rates.envFrames, mono.length, sampleRate)
  }
  if (wantBands) {
    let bandMax = 0
    let count = 0
    for (const part of scanned) {
      count += part.bands.length
      if (part.bandMax > bandMax) bandMax = part.bandMax
    }
    const data = new Uint8Array(count)
    let offset = 0
    for (const part of scanned) {
      data.set(part.bands, offset)
      offset += part.bands.length
    }
    normalizeBands(data, bandMax)
    built.bands = data
    built.bandRate = reportedRate(rates.bandFrames, mono.length, sampleRate)
  }
  return built
}
