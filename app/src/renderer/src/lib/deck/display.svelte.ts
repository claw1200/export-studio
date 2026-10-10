import type { WaveformMode } from '../waveform/data'
import { storedPercent } from './stored'
import { WaveLoader, type WaveSamples } from './waves.svelte'

/** How the deck's waveforms are drawn, kept between sessions, and the samples the detail view uses. */
export class WaveformDisplay {
  mode = $state<WaveformMode>((localStorage.getItem('waveformMode') as WaveformMode) || 'rgb')
  /** Detail of the engine's own waveform, 0 to 100; 0 draws rekordbox's analysis instead. */
  quality = $state(storedPercent('waveQuality', 30))
  /** How far the 3-band levels fall after a peak, 0 to 100 blocks. */
  fallBlocks = $state(storedPercent('fallBlocks', 40))
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)

  private readonly waves = new WaveLoader()

  /** `trackId` is the loaded track, for the engine to analyse. */
  constructor(private trackId: () => string | null) {}

  get samples(): WaveSamples | null {
    return this.waves.samples
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    localStorage.setItem('waveformMode', mode)
    this.reload()
  }

  setQuality(quality: number): void {
    const q = Math.min(100, Math.max(0, Math.round(quality)))
    if (this.quality === q) return
    const wasOff = this.quality <= 0
    this.quality = q
    localStorage.setItem('waveQuality', String(q))
    if (q <= 0 || wasOff) this.reload()
  }

  setFallBlocks(blocks: number): void {
    const n = Math.min(100, Math.max(0, Math.round(blocks)))
    if (this.fallBlocks === n) return
    this.fallBlocks = n
    localStorage.setItem('fallBlocks', String(n))
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    localStorage.setItem('waveformZoom', String(this.zoom))
  }

  /** Fetches the samples for the loaded track, or drops them when they are not drawn. */
  reload(): void {
    const id = this.trackId()
    if (!id || this.quality <= 0) this.waves.clear()
    else this.waves.load(id, this.mode === '3band')
  }
}
