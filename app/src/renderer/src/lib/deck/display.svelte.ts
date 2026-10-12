import { getSetting, setSetting } from '../settings'
import type { WaveformMode } from '../waveform/data'
import { storedPercent } from './stored'
import { WaveLoader, type WaveSamples } from './waves.svelte'

/** How the deck's waveforms are drawn, kept between sessions, and the samples the detail view uses. */
export class WaveformDisplay {
  mode = $state<WaveformMode>((getSetting('waveformMode') as WaveformMode) || 'rgb')
  /** Detail of the engine's own waveform, 0 to 100; 0 draws rekordbox's analysis instead. */
  quality = $state(storedPercent('waveQuality', 30))
  /** How far the 3-band levels fall after a peak, 0 to 100 blocks. */
  fallBlocks = $state(storedPercent('fallBlocks', 40))
  zoom = $state(Number(getSetting('waveformZoom')) || 1)

  private readonly waves = new WaveLoader()
  private readonly trackId: () => string | null
  private readonly isLoose: () => boolean

  /** `trackId` is the loaded track. `isLoose` is a file that is not in the collection. */
  constructor(trackId: () => string | null, isLoose: () => boolean) {
    this.trackId = trackId
    this.isLoose = isLoose
  }

  get samples(): WaveSamples | null {
    return this.waves.samples
  }

  /** True when the samples were generated at the resolution they should be drawn at. */
  get prepared(): boolean {
    return this.waves.prepared
  }

  /** Samples for the canvas. A loose file always draws its own; a collection track only while quality is on. */
  get paintSamples(): WaveSamples | null {
    if (this.prepared) return this.samples
    return this.quality > 0 ? this.samples : null
  }

  /**
   * Stride passed to the canvas. Loose samples are already as coarse as their quality, so every
   * entry is drawn. Collection samples are a full scan, thinned here.
   */
  get paintQuality(): number {
    return this.prepared ? 100 : this.quality
  }

  /** Fall for the samples on screen. The fast preview, and a loose file with quality off, use none. */
  get paintFall(): number {
    if (this.prepared && (this.waves.previewing || this.quality <= 0)) return 0
    return this.fallBlocks
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    setSetting('waveformMode', mode)
    this.reload()
  }

  setQuality(quality: number): void {
    const q = Math.min(100, Math.max(0, Math.round(quality)))
    if (this.quality === q) return
    const wasOff = this.quality <= 0
    this.quality = q
    setSetting('waveQuality', String(q))
    if (this.isLoose() || q <= 0 || wasOff) this.reload()
  }

  setFallBlocks(blocks: number): void {
    const n = Math.min(100, Math.max(0, Math.round(blocks)))
    if (this.fallBlocks === n) return
    this.fallBlocks = n
    setSetting('fallBlocks', String(n))
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    setSetting('waveformZoom', String(this.zoom))
  }

  /** Fetches the samples for the loaded track, or drops them when they are not drawn. */
  reload(): void {
    const id = this.trackId()
    if (!id) {
      this.waves.clear()
      return
    }
    if (this.isLoose()) {
      this.waves.loadLoose(id, this.mode === '3band', this.quality)
      return
    }
    if (this.quality <= 0) this.waves.clear()
    else this.waves.load(id, this.mode === '3band')
  }
}
