import type { WaveAnalysis } from '../../../../preload/audio'
import { Lru } from '../lru'

/** The decoded audio's envelope (and, for 3-band, its band peaks), as the sample waveform draws it. */
export type WaveSamples = WaveAnalysis

/** A coarse scan drawn while a finer one is still running. */
const PREVIEW_QUALITY = 2

const cache = new Lru<string, WaveSamples>(2)

function usable(samples: WaveSamples | null, bands: boolean): WaveSamples | null {
  return samples && (!bands || samples.bands) ? samples : null
}

/** Analyses the loaded track in the engine, keeping only the newest request. */
export class WaveLoader {
  samples = $state.raw<WaveSamples | null>(null)
  /** True when `samples` were generated at the resolution they are drawn at. */
  prepared = $state(false)
  /** True while a coarse preview is on screen and a finer scan is still running. */
  previewing = $state(false)
  private request = 0

  /** Shows `trackId`'s full waveform, analysing it unless it is cached with what `bands` asks for. */
  load(trackId: string, bands: boolean): void {
    const request = ++this.request
    this.prepared = false
    this.previewing = false
    const cached = cache.get(trackId) ?? null
    this.samples = cached
    if (cached && (!bands || cached.bands)) return
    void window.audio.analyse(trackId, bands, 100).then(
      (samples) => {
        if (!samples) return
        cache.set(trackId, samples)
        if (request === this.request) this.samples = samples
      },
      (err) => console.warn('[wave] analysis failed', err)
    )
  }

  /**
   * A file that is not in the collection. Draws a 2% scan first, then replaces it with one at
   * `quality` when that setting is on. Quality 0 keeps the coarse scan.
   */
  loadLoose(trackId: string, bands: boolean, quality: number): void {
    const request = ++this.request
    const target = quality > 0 ? Math.min(100, Math.round(quality)) : 0
    const previewKey = this.looseKey(trackId, bands, PREVIEW_QUALITY)
    const finalKey = this.looseKey(trackId, bands, target)
    const preview = usable(cache.get(previewKey) ?? null, bands)
    const final = target > 0 ? usable(cache.get(finalKey) ?? null, bands) : null
    this.prepared = true
    this.previewing = target > PREVIEW_QUALITY && !final
    this.samples = final ?? preview
    if (final) return
    void this.fillLoose(request, trackId, bands, target, previewKey, finalKey, !preview)
  }

  clear(): void {
    this.request++
    this.prepared = false
    this.previewing = false
    this.samples = null
  }

  private looseKey(trackId: string, bands: boolean, quality: number): string {
    return `${trackId}\0${bands ? 1 : 0}\0${quality}`
  }

  private async fillLoose(
    request: number,
    trackId: string,
    bands: boolean,
    target: number,
    previewKey: string,
    finalKey: string,
    needPreview: boolean
  ): Promise<void> {
    if (needPreview) {
      try {
        const fast = await window.audio.analyse(trackId, bands, PREVIEW_QUALITY)
        if (request !== this.request) return
        if (fast) {
          cache.set(previewKey, fast)
          if (!this.samples || this.previewing) this.samples = fast
        }
      } catch (err) {
        console.warn('[wave] analysis failed', err)
        if (request !== this.request) return
      }
    }
    if (request !== this.request) return
    if (target <= 0 || target === PREVIEW_QUALITY) {
      this.previewing = false
      return
    }
    try {
      const better = await window.audio.analyse(trackId, bands, target)
      if (request !== this.request) return
      if (better) {
        cache.set(finalKey, better)
        this.samples = better
        this.previewing = false
      }
    } catch (err) {
      console.warn('[wave] analysis failed', err)
    }
  }
}
