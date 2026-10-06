/** Mono PCM used to draw the RGB waveform at audio resolution. */

export interface WaveSamples {
  /** samples per second of audio */
  rate: number
  /** signed mono samples, −32767…32767 */
  data: Int16Array
  /** largest absolute sample, at least 1 */
  peak: number
}

const TARGET_RATE = 16_000
/** Caps a long mix at about 12 MB of samples. */
const MAX_SAMPLES = 6_000_000

const cache = new Map<string, WaveSamples>()
let context: AudioContext | null = null

export function cachedWaveSamples(url: string): WaveSamples | null {
  const hit = cache.get(url)
  if (!hit) return null
  cache.delete(url)
  cache.set(url, hit)
  return hit
}

function remember(url: string, samples: WaveSamples): void {
  cache.delete(url)
  cache.set(url, samples)
  while (cache.size > 2) cache.delete(cache.keys().next().value!)
}

/**
 * Decodes the track and keeps the loudest sample in each window, so zooming in
 * follows the real wave instead of the 150 Hz analysis bars.
 */
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
    remember(url, samples)
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
