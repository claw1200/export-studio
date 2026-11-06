import { getSetting, setSetting } from '../settings'
import type { WaveformMode } from '../waveform/data'
import { storedPercent } from './stored'
import { WaveLoader, type WaveSamples } from './waves.svelte'

/** What changed in the waveform preferences, so each deck can decide whether to fetch again. */
type WaveformChange = { kind: 'mode' } | { kind: 'quality'; wasOff: boolean }

/** How every deck's waveforms are drawn, kept between sessions. */
export class WaveformPrefs {
  mode = $state<WaveformMode>((getSetting('waveformMode') as WaveformMode) || 'rgb')
  /** Detail of the engine's own waveform, 0 to 100; 0 draws rekordbox's analysis instead. */
  quality = $state(storedPercent('waveQuality', 30))
  /** How far the 3-band levels fall after a peak, 0 to 100 blocks. */
  fallBlocks = $state(storedPercent('fallBlocks', 40))
  zoom = $state(Number(getSetting('waveformZoom')) || 1)

  private listeners = new Set<(change: WaveformChange) => void>()

  /** Called when the mode or quality changes. Returns the unsubscribe. */
  subscribe(listener: (change: WaveformChange) => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    setSetting('waveformMode', mode)
    this.emit({ kind: 'mode' })
  }

  setQuality(quality: number): void {
    const q = Math.min(100, Math.max(0, Math.round(quality)))
    if (this.quality === q) return
    const wasOff = this.quality <= 0
    this.quality = q
    setSetting('waveQuality', String(q))
    this.emit({ kind: 'quality', wasOff })
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

  private emit(change: WaveformChange): void {
    for (const listener of this.listeners) listener(change)
  }
}

/** One deck's waveform samples, drawn as the shared preferences say. */
export class WaveformDisplay {
  private readonly waves = new WaveLoader()
  private readonly trackId: () => string | null
  private readonly isLoose: () => boolean

  /** `trackId` is the loaded track. `isLoose` is a file that is not in the collection. */
  constructor(
    private readonly prefs: WaveformPrefs,
    trackId: () => string | null,
    isLoose: () => boolean
  ) {
    this.trackId = trackId
    this.isLoose = isLoose
    prefs.subscribe((change) => {
      if (change.kind === 'mode') this.reload()
      else if (this.isLoose() || prefs.quality <= 0 || change.wasOff) this.reload()
    })
  }

  get mode(): WaveformMode {
    return this.prefs.mode
  }

  get quality(): number {
    return this.prefs.quality
  }

  get fallBlocks(): number {
    return this.prefs.fallBlocks
  }

  get zoom(): number {
    return this.prefs.zoom
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
    this.prefs.setMode(mode)
  }

  setQuality(quality: number): void {
    this.prefs.setQuality(quality)
  }

  setFallBlocks(blocks: number): void {
    this.prefs.setFallBlocks(blocks)
  }

  setZoom(zoom: number): void {
    this.prefs.setZoom(zoom)
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
