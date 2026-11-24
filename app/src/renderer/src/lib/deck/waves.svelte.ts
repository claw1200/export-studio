import type { WaveAnalysis } from '../../../../preload/audio'
import { Lru } from '../lru'

/** The decoded audio's envelope (and, for 3-band, its band peaks), as the sample waveform draws it. */
export type WaveSamples = WaveAnalysis

const FULL_QUALITY = 100
/** A file outside the collection has nothing else to draw, so waveform quality off still scans this finely. */
const COARSEST_LOOSE_QUALITY = 2

const cache = new Lru<string, WaveSamples>(2)

/** Forget every analysed waveform, e.g. after a track was pointed at a different audio file. */
export function clearWaveCache(): void {
  cache.clear()
}

function looseKey(trackId: string, bands: boolean, quality: number): string {
  return `${trackId}\0${bands ? 1 : 0}\0${quality}`
}

/** Analyses the loaded track in the engine, keeping only the newest request. */
export class WaveLoader {
  samples = $state.raw<WaveSamples | null>(null)
  /** True when `samples` were generated at the resolution they are drawn at. */
  prepared = $state(false)
  private request = 0

  /** Shows `trackId`'s full waveform, analysing it unless it is cached with what `bands` asks for. */
  load(trackId: string, bands: boolean): void {
    const request = ++this.request
    this.prepared = false
    const cached = cache.get(trackId) ?? null
    this.samples = cached
    if (cached && (!bands || cached.bands)) return
    this.analyse(request, trackId, bands, FULL_QUALITY, trackId)
  }

  /** A file that is not in the collection, scanned at `quality`. */
  loadLoose(trackId: string, bands: boolean, quality: number): void {
    const request = ++this.request
    const scanQuality = Math.min(FULL_QUALITY, Math.max(COARSEST_LOOSE_QUALITY, Math.round(quality)))
    const key = looseKey(trackId, bands, scanQuality)
    const cached = cache.get(key) ?? null
    this.prepared = true
    this.samples = cached
    if (!cached) this.analyse(request, trackId, bands, scanQuality, key)
  }

  clear(): void {
    this.request++
    this.prepared = false
    this.samples = null
  }

  private analyse(request: number, trackId: string, bands: boolean, quality: number, cacheKey: string): void {
    void window.audio.analyse(trackId, bands, quality).then(
      (samples) => {
        if (!samples) return
        cache.set(cacheKey, samples)
        if (request === this.request) this.samples = samples
      },
      (err) => console.warn('[wave] analysis failed', err)
    )
  }
}
