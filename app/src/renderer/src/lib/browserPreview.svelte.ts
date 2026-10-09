import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { api } from './api'
import { HOT_CUE_KINDS } from './format'
import type { Cue, TrackPreview } from './types'

const BATCH = 64
const CACHE = 400

export interface BrowserWave {
  bands?: Uint8Array
  bandLength: number
  heights?: Uint8Array
  colors?: Uint8Array
  rgbLength: number
}

export interface BrowserPreview {
  memory: boolean
  hot: boolean
  phrase: boolean
  cues: Cue[]
  wave: BrowserWave | null
}

const EMPTY: BrowserPreview = { memory: false, hot: false, phrase: false, cues: [], wave: null }

function bytes(data: string): Uint8Array {
  const bin = atob(data)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

function decodeWave(waveform: TrackPreview['waveform']): BrowserWave | null {
  if (!waveform) return null
  const wave: BrowserWave = { bandLength: 0, rgbLength: 0 }
  if (waveform.bands && waveform.bandLength) {
    wave.bands = bytes(waveform.bands)
    wave.bandLength = waveform.bandLength
  }
  if (waveform.heights && waveform.colors && waveform.rgbLength) {
    wave.heights = bytes(waveform.heights)
    wave.colors = bytes(waveform.colors)
    wave.rgbLength = waveform.rgbLength
  }
  return wave.bands || wave.heights ? wave : null
}

function decode(row: TrackPreview): BrowserPreview {
  return {
    memory: row.memory,
    hot: row.hot,
    phrase: row.phrase,
    cues: Array.isArray(row.cues) ? row.cues : [],
    wave: decodeWave(row.waveform)
  }
}

/** Waveform thumbnails and cue stars for the rows currently on screen. */
export class BrowserPreviews {
  revision = $state(0)
  private epoch = 0
  private cache = new SvelteMap<string, BrowserPreview>()
  private loaded = new SvelteSet<string>()
  private inflight = new SvelteSet<string>()
  private generation = new SvelteMap<string, number>()
  private cueOverride = new SvelteMap<string, { memory: boolean; hot: boolean; cues: Cue[] }>()
  private queued: string[] = []
  private timer: ReturnType<typeof setTimeout> | undefined
  private flushing = false

  /** Keeps library sort counts in step when cues are written. */
  onCueChange: ((id: string, cues: Cue[]) => void) | null = null

  /** The loaded preview. Reads `revision`, so a row updates when a batch arrives. */
  preview(id: string): BrowserPreview | undefined {
    void this.revision
    return this.cache.get(id)
  }

  request(ids: string[]): void {
    const missing = ids.filter((id) => !this.loaded.has(id) && !this.inflight.has(id))
    if (!missing.length) return
    this.queued = missing
    clearTimeout(this.timer)
    this.timer = setTimeout(() => void this.flush(), 40)
  }

  forget(ids?: string[]): void {
    if (!ids) {
      this.epoch++
      this.cache.clear()
      this.loaded.clear()
      this.queued = []
      this.inflight.clear()
      this.generation.clear()
      this.cueOverride.clear()
      this.revision++
      return
    }
    for (const id of ids) {
      this.generation.set(id, (this.generation.get(id) ?? 0) + 1)
      this.loaded.delete(id)
      this.cache.delete(id)
      this.cueOverride.delete(id)
    }
    this.queued = this.queued.filter((id) => !ids.includes(id))
    this.revision++
  }

  noteCues(id: string, cues: Cue[]): void {
    const marks = {
      memory: cues.some((cue) => cue.kind === 'memory'),
      hot: cues.some((cue) => cue.kind === 'hot' && HOT_CUE_KINDS.includes(cue.slot)),
      cues
    }
    this.cueOverride.set(id, marks)
    this.onCueChange?.(id, cues)
    const row = this.cache.get(id)
    if (!row) return
    this.cache.set(id, { ...row, ...marks })
    this.revision++
  }

  private remember(id: string, row: BrowserPreview): void {
    this.cache.delete(id)
    this.cache.set(id, row)
    while (this.cache.size > CACHE) {
      const oldest = this.cache.keys().next().value
      if (oldest === undefined) break
      this.cache.delete(oldest)
      this.loaded.delete(oldest)
    }
  }

  private async flush(): Promise<void> {
    if (this.flushing) return
    const ids = this.queued.slice(0, BATCH)
    if (!ids.length) return
    this.queued = this.queued.filter((id) => !ids.includes(id))
    const epoch = this.epoch
    const gens: Record<string, number> = {}
    for (const id of ids) gens[id] = this.generation.get(id) ?? 0
    for (const id of ids) this.inflight.add(id)
    this.flushing = true
    try {
      const rows = await api.trackPreviews(ids)
      if (this.epoch !== epoch) return
      const byId: Record<string, TrackPreview> = {}
      for (const row of rows) byId[row.id] = row
      let changed = false
      for (const id of ids) {
        if ((this.generation.get(id) ?? 0) !== gens[id]) continue
        const decoded = byId[id] ? decode(byId[id]) : EMPTY
        const marks = this.cueOverride.get(id)
        this.remember(id, marks ? { ...decoded, ...marks } : decoded)
        this.loaded.add(id)
        changed = true
      }
      if (changed) this.revision++
    } catch {
      // Left unloaded, so the next scroll asks again.
    } finally {
      if (this.epoch === epoch) for (const id of ids) this.inflight.delete(id)
      this.flushing = false
      if (this.queued.length) void this.flush()
    }
  }
}

export const browserPreviews = new BrowserPreviews()
