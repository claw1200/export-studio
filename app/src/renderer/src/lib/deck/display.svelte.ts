import { Listeners } from '../listeners'
import { getSetting, setSetting } from '../settings'
import type { WaveformMode } from '../waveform/data'
import { type Percent, storedNumber, storedPercent } from './stored'
import { WaveLoader, type WaveSamples } from './waves.svelte'

/** What changed in the waveform preferences, so each deck can decide whether to fetch again. */
type WaveformChange = { kind: 'mode' } | { kind: 'quality'; mode: WaveformMode; wasOff: boolean }

const QUALITY_KEYS: Record<WaveformMode, string> = { rgb: 'waveQualityRgb', '3band': 'waveQuality3band' }
const DEFAULT_QUALITY = storedPercent('waveQuality', 30)
export const MAX_QUALITY: Record<WaveformMode, Percent> = { rgb: 100, '3band': 25 }

function storedQuality(mode: WaveformMode): Percent {
  return Math.min(MAX_QUALITY[mode], storedPercent(QUALITY_KEYS[mode], DEFAULT_QUALITY))
}

export const MAX_FALL_BLOCKS_PER_QUALITY = 3
export const FALL_BLOCKS_PER_QUALITY_STEP = 0.05

/** How every deck's waveforms are drawn, kept between sessions. */
export class WaveformPrefs {
  mode = $state<WaveformMode>((getSetting('waveformMode') as WaveformMode) || '3band')
  qualities = $state<Record<WaveformMode, Percent>>({ rgb: storedQuality('rgb'), '3band': storedQuality('3band') })
  fallBlocksPerQuality = $state(storedNumber('fallBlocksPerQuality', 1.3, 0, MAX_FALL_BLOCKS_PER_QUALITY))
  zoom = $state(Number(getSetting('waveformZoom')) || 1)

  private changed = new Listeners<[change: WaveformChange]>()

  subscribe(listener: (change: WaveformChange) => void): () => void {
    return this.changed.add(listener)
  }

  get quality(): Percent {
    return this.qualities[this.mode]
  }

  get fallBlocks(): number {
    return Math.round(this.fallBlocksPerQuality * this.qualities['3band'])
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    setSetting('waveformMode', mode)
    this.emit({ kind: 'mode' })
  }

  setQuality(mode: WaveformMode, quality: Percent): void {
    const q = Math.min(MAX_QUALITY[mode], Math.max(0, Math.round(quality)))
    if (this.qualities[mode] === q) return
    const wasOff = this.qualities[mode] <= 0
    this.qualities[mode] = q
    setSetting(QUALITY_KEYS[mode], String(q))
    this.emit({ kind: 'quality', mode, wasOff })
  }

  setFallBlocksPerQuality(scale: number): void {
    const steps = Math.round(Math.min(MAX_FALL_BLOCKS_PER_QUALITY, Math.max(0, scale)) / FALL_BLOCKS_PER_QUALITY_STEP)
    const snapped = Number((steps * FALL_BLOCKS_PER_QUALITY_STEP).toFixed(2))
    if (this.fallBlocksPerQuality === snapped) return
    this.fallBlocksPerQuality = snapped
    setSetting('fallBlocksPerQuality', String(snapped))
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    setSetting('waveformZoom', String(this.zoom))
  }

  private emit(change: WaveformChange): void {
    this.changed.notify(change)
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
      else if (change.mode !== prefs.mode) return
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
