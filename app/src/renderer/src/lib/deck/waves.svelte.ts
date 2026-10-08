import type { WaveAnalysis } from '../../../../preload/audio'
import { Lru } from '../lru'

/** The decoded audio's envelope (and, for 3-band, its band peaks), as the sample waveform draws it. */
export type WaveSamples = WaveAnalysis

const cache = new Lru<string, WaveSamples>(2)

/** Analyses the loaded track in the engine, keeping only the newest request. */
export class WaveLoader {
  samples = $state.raw<WaveSamples | null>(null)
  private request = 0

  /** Shows `trackId`'s waveform, analysing it unless it is cached with what `bands` asks for. */
  load(trackId: string, bands: boolean): void {
    const request = ++this.request
    const cached = cache.get(trackId) ?? null
    this.samples = cached
    if (cached && (!bands || cached.bands)) return
    void window.audio.analyse(trackId, bands).then(
      (samples) => {
        if (!samples) return
        cache.set(trackId, samples)
        if (request === this.request) this.samples = samples
      },
      (err) => console.warn('[wave] analysis failed', err)
    )
  }

  clear(): void {
    this.request++
    this.samples = null
  }
}
