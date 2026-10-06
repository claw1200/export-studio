import { Lru } from './lru'

export interface WaveSamples {
  rate: number
  data: Int16Array
  peak: number
}

const TARGET_RATE = 16_000
const BYTES_PER_SAMPLE = 2
const MAX_BYTES = 12_000_000
const MAX_SAMPLES = MAX_BYTES / BYTES_PER_SAMPLE
const CACHE_SIZE = 2

const cache = new Lru<string, WaveSamples>(CACHE_SIZE)
let context: AudioContext | null = null

export function cachedWaveSamples(url: string): WaveSamples | null {
  return cache.get(url) ?? null
}

export async function loadWaveSamples(url: string, signal: AbortSignal): Promise<WaveSamples | null> {
  const hit = cachedWaveSamples(url)
  if (hit) return hit
  try {
    const res = await fetch(url, { signal })
    if (!res.ok) return null
    const buf = await res.arrayBuffer()
    if (signal.aborted) return null
    context ??= new AudioContext()
    const audio = await context.decodeAudioData(buf)
    if (signal.aborted) return null
    const samples = await downsample(audio, signal)
    if (!samples || signal.aborted) return null
    cache.set(url, samples)
    return samples
  } catch (err) {
    if (signal.aborted || (err as Error).name === 'AbortError') return null
    console.warn('[waveform] could not read audio', err)
    return null
  }
}

async function downsample(audio: AudioBuffer, signal: AbortSignal): Promise<WaveSamples | null> {
  const left = audio.getChannelData(0)
  if (left.length < 2) return null
  const right = audio.numberOfChannels > 1 ? audio.getChannelData(1) : null
  const duration = left.length / audio.sampleRate
  const rate = Math.min(TARGET_RATE, MAX_SAMPLES / duration)
  const step = audio.sampleRate / rate
  const n = Math.floor(left.length / step)
  if (n < 2) return null

  const data = new Int16Array(n)
  let peak = 1
  for (let k = 0; k < n;) {
    if (signal.aborted) return null
    const stop = Math.min(n, k + 250_000)
    for (; k < stop; k++) {
      const start = Math.floor(k * step)
      const end = Math.min(left.length, Math.max(start + 1, Math.floor((k + 1) * step)))
      let s = 0
      for (let j = start; j < end; j++) {
        const v = right ? (left[j] + right[j]) * 0.5 : left[j]
        if (Math.abs(v) > Math.abs(s)) s = v
      }
      const q = Math.max(-32767, Math.min(32767, Math.round(s * 32767)))
      data[k] = q
      const a = q < 0 ? -q : q
      if (a > peak) peak = a
    }
    if (k < n) await new Promise((resolve) => setTimeout(resolve, 0))
  }
  return { rate: (n * audio.sampleRate) / left.length, data, peak }
}
