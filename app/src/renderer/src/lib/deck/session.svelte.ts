import { api } from '../api'
import { Lru } from '../lru'
import type { Track, TrackDetail } from '../types'
import { decodeWaveform, type Waveform } from '../waveform/data'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Lru<string, Promise<DetailEntry>>(24)

export function loadDetail(id: string): Promise<DetailEntry> {
  const hit = detailCache.get(id)
  if (hit) return hit
  const entry = api.trackDetail(id).then((detail) => ({ detail, waveform: decodeWaveform(detail) }))
  entry.catch(() => detailCache.delete(id))
  detailCache.set(id, entry)
  return entry
}

export function clearDetailCache(): void {
  detailCache.clear()
}

/** The track on the deck and its library detail: cues, beatgrid and the analysed waveform. */
export class TrackSession {
  track = $state.raw<Track | null>(null)
  detail = $state.raw<TrackDetail | null>(null)
  waveform = $state.raw<Waveform | null>(null)
  loading = $state(false)
  /** A library write is in flight. Only one runs at a time. */
  editing = $state(false)
  editError = $state<string | null>(null)
  error = $state<string | null>(null)
  /** Bumped on every load, so late results for an earlier track are dropped. */
  token = 0

  async open(track: Track): Promise<void> {
    const token = ++this.token
    this.track = track
    this.detail = null
    this.waveform = null
    this.error = null
    this.editError = null
    this.loading = true
    try {
      const { detail, waveform } = await loadDetail(track.id)
      if (token !== this.token) return
      this.detail = detail
      this.waveform = waveform
    } catch (e) {
      if (token === this.token) this.error = (e as Error).message
    } finally {
      if (token === this.token) this.loading = false
    }
  }

  async refresh(): Promise<void> {
    const id = this.track?.id
    if (!id) return
    const token = this.token
    clearDetailCache()
    try {
      const { detail, waveform } = await loadDetail(id)
      if (token !== this.token || this.track?.id !== id) return
      this.detail = detail
      this.waveform = waveform
    } catch {
      return
    }
  }

  /**
   * Runs one library write against the current track and folds its result into the detail.
   * Returns null when the write could not start, failed, or the track changed meanwhile.
   */
  async edit<T>(
    write: (id: string) => Promise<T>,
    apply: (detail: TrackDetail, result: T) => TrackDetail
  ): Promise<T | null> {
    const id = this.track?.id
    if (!id || !this.detail || this.editing) return null
    this.editing = true
    this.editError = null
    try {
      const result = await write(id)
      if (this.track?.id !== id || !this.detail) {
        detailCache.delete(id)
        return null
      }
      this.detail = apply(this.detail, result)
      detailCache.set(id, Promise.resolve({ detail: this.detail, waveform: this.waveform }))
      return result
    } catch (e) {
      if (this.track?.id === id) this.editError = (e as Error).message
      return null
    } finally {
      this.editing = false
    }
  }
}
