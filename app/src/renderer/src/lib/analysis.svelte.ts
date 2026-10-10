import type { AnalysisOptions, AnalysisProgress, GridMode } from '../../../preload/analysis'
import type { Track } from './types'

/** The tempo ranges rekordbox offers, as the lowest and highest BPM the analysis may pick. */
export const BPM_RANGES: readonly (readonly [number, number])[] = [
  [58, 115],
  [70, 180],
  [78, 155],
  [88, 175],
  [98, 195],
  [118, 235]
]

export const MAX_CONCURRENCY = 4

export interface AnalysisSettings {
  /** Analyse tracks as soon as they are imported. */
  auto: boolean
  gridMode: GridMode
  minBpm: number
  maxBpm: number
  highPrecision: boolean
  roundBpm: boolean
  key: boolean
  overwrite: boolean
  concurrency: number
}

const STORAGE_KEY = 'analysisSettings'

const DEFAULTS: AnalysisSettings = {
  auto: true,
  gridMode: 'normal',
  minBpm: 88,
  maxBpm: 175,
  highPrecision: true,
  roundBpm: true,
  key: true,
  overwrite: false,
  concurrency: 2
}

function load(): AnalysisSettings {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<AnalysisSettings>
    const out = { ...DEFAULTS }
    for (const key of Object.keys(DEFAULTS) as (keyof AnalysisSettings)[]) {
      if (typeof stored[key] === typeof DEFAULTS[key]) Object.assign(out, { [key]: stored[key] })
    }
    if (!BPM_RANGES.some(([lo, hi]) => lo === out.minBpm && hi === out.maxBpm)) {
      out.minBpm = DEFAULTS.minBpm
      out.maxBpm = DEFAULTS.maxBpm
    }
    if (out.gridMode !== 'normal' && out.gridMode !== 'dynamic') out.gridMode = DEFAULTS.gridMode
    out.concurrency = Math.max(1, Math.min(MAX_CONCURRENCY, Math.round(out.concurrency)))
    return out
  } catch {
    return { ...DEFAULTS }
  }
}

export function analysable(track: Track): boolean {
  return !track.missing && !track.streaming && !track.id.startsWith('usb:')
}

/** Analysis settings, and the progress of the analysis running in the main process. */
export class Analysis {
  settings = $state<AnalysisSettings>(load())
  progress = $state.raw<AnalysisProgress | null>(null)
  running = $derived(!!this.progress && this.progress.done + this.progress.failed < this.progress.total)
  /** The most recent failure in this batch. */
  lastError = $state<string | null>(null)
  private finishListeners = new Set<(track: Track | null, id: string, error?: string) => void>()
  private batchListeners = new Set<(analysed: number, failed: number, error: string | null) => void>()

  constructor() {
    window.api.onAnalysis((progress) => {
      const starting = !this.running
      this.progress = progress
      if (starting) this.lastError = null
      const finished = progress.finished
      if (!finished) return
      if (finished.error) this.lastError = finished.error
      for (const listener of this.finishListeners) listener((finished.track as Track) ?? null, finished.id, finished.error)
      if (!this.running) {
        for (const listener of this.batchListeners) listener(progress.done, progress.failed, this.lastError)
      }
    })
  }

  /** Called when every queued track has been analysed or has failed. */
  onBatchDone(listener: (analysed: number, failed: number, error: string | null) => void): () => void {
    this.batchListeners.add(listener)
    return () => this.batchListeners.delete(listener)
  }

  set<K extends keyof AnalysisSettings>(key: K, value: AnalysisSettings[K]): void {
    this.settings[key] = value
    localStorage.setItem(STORAGE_KEY, JSON.stringify($state.snapshot(this.settings)))
  }

  setRange(minBpm: number, maxBpm: number): void {
    this.set('minBpm', minBpm)
    this.set('maxBpm', maxBpm)
  }

  options(): AnalysisOptions {
    const s = this.settings
    return {
      grid: true,
      key: s.key,
      gridMode: s.gridMode,
      minBpm: s.minBpm,
      maxBpm: s.maxBpm,
      highPrecision: s.highPrecision,
      roundBpm: s.roundBpm,
      overwrite: s.overwrite
    }
  }

  /** Called with each track as its analysis is saved, or with null and the error when it failed. */
  onFinished(listener: (track: Track | null, id: string, error?: string) => void): () => void {
    this.finishListeners.add(listener)
    return () => this.finishListeners.delete(listener)
  }

  /** Re-analysing tracks that all have an analysis replaces it, whatever `overwrite` says. */
  analyse(tracks: Track[]): void {
    const wanted = tracks.filter(analysable)
    if (!wanted.length) return
    const options = this.options()
    if (wanted.every((t) => t.analysed)) options.overwrite = true
    void window.api.analyse(
      wanted.map((t) => ({ id: t.id, options })),
      this.settings.concurrency
    )
  }

  /** Analyse freshly imported tracks that have no analysis yet, when that is switched on. */
  afterImport(tracks: Track[]): void {
    if (this.settings.auto) this.analyse(tracks.filter((t) => !t.analysed))
  }

  cancel(): void {
    void window.api.cancelAnalysis()
  }
}
