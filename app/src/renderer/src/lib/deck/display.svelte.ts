import { Listeners } from '../listeners'
import { getSetting, setSetting } from '../settings'
import type { Waveform, WaveformMode } from '../waveform/data'
import { gradientColors, pendingColors } from '../waveform/gradientColors'
import { rgbGradient } from '../waveform/rgbGradient.svelte'
import { type Percent, storedFlag, storeFlag, storedNumber, storedPercent } from './stored'
import { WaveLoader, type WaveSamples } from './waves.svelte'

/** What changed in the waveform preferences, so each deck can decide whether to fetch again. */
type WaveformChange =
  { kind: 'mode' } | { kind: 'rgbColorSource' } | { kind: 'quality'; mode: WaveformMode; wasOff: boolean }

const QUALITY_KEYS: Record<WaveformMode, string> = { rgb: 'waveQualityRgb', '3band': 'waveQuality3band' }
export const MAX_QUALITY: Record<WaveformMode, Percent> = { rgb: 100, '3band': 25 }
export const DEFAULT_QUALITIES: Record<WaveformMode, Percent> = { rgb: 30, '3band': 25 }
export const DEFAULT_FALL_BLOCKS_PER_QUALITY = 1.3
export const DEFAULT_CUSTOM_RGB_COLORS = true
const LEGACY_QUALITY_KEY = 'waveQuality'

function storedQuality(mode: WaveformMode): Percent {
  const fallback = storedPercent(LEGACY_QUALITY_KEY, DEFAULT_QUALITIES[mode])
  return Math.min(MAX_QUALITY[mode], storedPercent(QUALITY_KEYS[mode], fallback))
}

export const MAX_FALL_BLOCKS_PER_QUALITY = 10
export const FALL_BLOCKS_PER_QUALITY_STEP = 0.05

export type WaveformView = 'detail' | 'overview'

const HEIGHT_SCALE_KEYS: Record<WaveformView, string> = {
  detail: 'waveHeightScaleDetail',
  overview: 'waveHeightScaleOverview'
}
export const MIN_HEIGHT_SCALE = 0.5
const DEFAULT_HEIGHT_SCALE = 0.85
export const HEIGHT_SCALE_STEP = 0.05

function storedHeightScale(view: WaveformView): number {
  return storedNumber(HEIGHT_SCALE_KEYS[view], DEFAULT_HEIGHT_SCALE, MIN_HEIGHT_SCALE, 1)
}

function snapToStep(value: number, min: number, max: number, step: number): number {
  const steps = Math.round(Math.min(max, Math.max(min, value)) / step)
  return Number((steps * step).toFixed(2))
}

/** How every deck's waveforms are drawn, kept between sessions. */
export class WaveformPrefs {
  mode = $state<WaveformMode>((getSetting('waveformMode') as WaveformMode) || '3band')
  qualities = $state<Record<WaveformMode, Percent>>({ rgb: storedQuality('rgb'), '3band': storedQuality('3band') })
  fallBlocksPerQuality = $state(
    storedNumber('fallBlocksPerQuality', DEFAULT_FALL_BLOCKS_PER_QUALITY, 0, MAX_FALL_BLOCKS_PER_QUALITY)
  )
  customRgbColors = $state(storedFlag('waveformCustomRgbColors', DEFAULT_CUSTOM_RGB_COLORS))
  heightScales = $state<Record<WaveformView, number>>({
    detail: storedHeightScale('detail'),
    overview: storedHeightScale('overview')
  })
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

  get rgbColorsFromAudio(): boolean {
    return this.mode === 'rgb' && this.customRgbColors
  }

  get needsBands(): boolean {
    return this.mode === '3band' || this.rgbColorsFromAudio
  }

  /** One colour per waveform entry: rekordbox's, or with custom colours on, the gradient over `bands` (grey until in). */
  rgbColors(
    rekordboxColors: Uint8Array | undefined,
    entriesPerSecond: number,
    entries: number,
    bands: Uint8Array | undefined,
    bandsPerSecond: number | undefined
  ): Uint8Array | undefined {
    if (!this.customRgbColors) return rekordboxColors
    const { stopsLowToHigh } = rgbGradient
    const computed =
      bands && bandsPerSecond ? gradientColors(bands, bandsPerSecond, entriesPerSecond, entries, stopsLowToHigh) : null
    return computed ?? pendingColors(entries)
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
    const snapped = snapToStep(scale, 0, MAX_FALL_BLOCKS_PER_QUALITY, FALL_BLOCKS_PER_QUALITY_STEP)
    if (this.fallBlocksPerQuality === snapped) return
    this.fallBlocksPerQuality = snapped
    setSetting('fallBlocksPerQuality', String(snapped))
  }

  setCustomRgbColors(on: boolean): void {
    if (this.customRgbColors === on) return
    this.customRgbColors = on
    storeFlag('waveformCustomRgbColors', on)
    this.emit({ kind: 'rgbColorSource' })
  }

  setHeightScale(view: WaveformView, scale: number): void {
    const snapped = snapToStep(scale, MIN_HEIGHT_SCALE, 1, HEIGHT_SCALE_STEP)
    if (this.heightScales[view] === snapped) return
    this.heightScales[view] = snapped
    setSetting(HEIGHT_SCALE_KEYS[view], String(snapped))
  }

  get heightScalesAreDefault(): boolean {
    return Object.values(this.heightScales).every((scale) => scale === DEFAULT_HEIGHT_SCALE)
  }

  resetHeightScales(): void {
    this.setHeightScale('detail', DEFAULT_HEIGHT_SCALE)
    this.setHeightScale('overview', DEFAULT_HEIGHT_SCALE)
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
      if (change.kind === 'mode' || change.kind === 'rgbColorSource') this.reload()
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

  get heightScales(): Record<WaveformView, number> {
    return this.prefs.heightScales
  }

  get samples(): WaveSamples | null {
    return this.waves.samples
  }

  rgbColors(wf: Waveform): Uint8Array | undefined {
    const samples = this.waves.samples
    return this.prefs.rgbColors(wf.rgbColors, wf.rate, wf.length, samples?.bands, samples?.bandRate)
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
      this.waves.loadLoose(id, this.prefs.needsBands, this.quality)
      return
    }
    if (this.quality <= 0 && !this.prefs.rgbColorsFromAudio) this.waves.clear()
    else this.waves.load(id, this.prefs.needsBands)
  }
}
