import { api, audioUrl } from './api'
import type { Track, TrackDetail } from './types'
import { decodeWaveform, type Waveform, type WaveformMode } from './waveform'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Map<string, Promise<DetailEntry>>()
const CACHE_SIZE = 24

export function loadDetail(id: string): Promise<DetailEntry> {
  const hit = detailCache.get(id)
  if (hit) {
    detailCache.delete(id)
    detailCache.set(id, hit)
    return hit
  }
  const entry = api.trackDetail(id).then((detail) => ({ detail, waveform: decodeWaveform(detail) }))
  entry.catch(() => detailCache.delete(id))
  detailCache.set(id, entry)
  if (detailCache.size > CACHE_SIZE) detailCache.delete(detailCache.keys().next().value!)
  return entry
}

export function clearDetailCache(): void {
  detailCache.clear()
}

export class Player {
  track = $state.raw<Track | null>(null)
  detail = $state.raw<TrackDetail | null>(null)
  waveform = $state.raw<Waveform | null>(null)
  loading = $state(false)
  playing = $state(false)
  position = $state(0)
  duration = $state(0)
  audioError = $state<string | null>(null)
  mode = $state<WaveformMode>((localStorage.getItem('waveformMode') as WaveformMode) || 'rgb')
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)

  private audio = new Audio()
  private raf = 0
  private token = 0
  /** Playhead is following a gesture, so playback must not overwrite it. */
  private scrubbing = false
  /** Target of the seek the element is currently performing. */
  private sent: number | null = null
  /** Seek made before the element knew its duration. */
  private pending: number | null = null

  constructor() {
    this.audio.preload = 'auto'
    this.audio.addEventListener('play', () => {
      this.playing = true
      this.tick()
    })
    this.audio.addEventListener('pause', () => {
      this.playing = false
      cancelAnimationFrame(this.raf)
      if (!this.scrubbing) this.position = this.audio.currentTime
    })
    this.audio.addEventListener('ended', () => (this.playing = false))
    this.audio.addEventListener('seeked', () => {
      const sent = this.sent
      this.sent = null
      if (sent != null && Math.abs(this.position - sent) >= 0.05) this.apply(this.position)
    })
    this.audio.addEventListener('loadedmetadata', () => {
      if (Number.isFinite(this.audio.duration)) this.duration = this.audio.duration
      if (this.pending != null && !this.scrubbing) {
        const t = this.pending
        this.pending = null
        this.apply(t)
      }
    })
    this.audio.addEventListener('error', () => {
      // Replacing src aborts the previous load. That is not a failure of the new one.
      if (this.audio.error?.code === MediaError.MEDIA_ERR_ABORTED) return
      const ext = this.track?.fileType ?? ''
      this.audioError =
        this.detail && !this.detail.fileExists
          ? 'Audio file not found'
          : `Preview not supported for ${ext || 'this file type'}`
    })
  }

  private tick = (): void => {
    if (!this.scrubbing) this.position = this.audio.currentTime
    if (!this.audio.paused) this.raf = requestAnimationFrame(this.tick)
  }

  /** Move the playhead. The element follows when it is free. */
  scrub(seconds: number): void {
    this.scrubbing = true
    this.apply(seconds)
  }

  /** The gesture has ended. Land on the last playhead position. */
  commit(): void {
    if (!this.scrubbing) return
    this.scrubbing = false
    this.apply(this.position)
  }

  /** Discrete jump: cue, keyboard, inspector. */
  seek(seconds: number): void {
    this.scrubbing = false
    this.apply(seconds)
  }

  private mediaEnd(): number {
    const ranges = this.audio.seekable
    if (ranges.length > 0) return ranges.end(ranges.length - 1)
    if (Number.isFinite(this.audio.duration) && this.audio.duration > 0) return this.audio.duration
    return this.duration
  }

  /** The byte at `duration` is past the last sample, so stay just inside the file. */
  private playable(seconds: number): number {
    const end = this.mediaEnd()
    const max = Number.isFinite(end) && end > 0.05 ? end - 0.001 : Math.max(0, end)
    return Math.max(0, Math.min(seconds, max || seconds))
  }

  private apply(seconds: number): void {
    const t = this.playable(seconds)
    this.position = t
    if (this.audio.readyState < HTMLMediaElement.HAVE_METADATA) {
      this.pending = t
      return
    }
    this.pending = null
    if (this.sent != null || Math.abs(this.audio.currentTime - t) < 0.05) return
    this.sent = t
    this.audio.currentTime = t
  }

  async load(track: Track, autoplay = false): Promise<void> {
    if (this.track?.id === track.id) {
      if (autoplay) this.play()
      return
    }
    const token = ++this.token
    this.audio.pause()
    this.scrubbing = false
    this.sent = null
    this.pending = null
    this.track = track
    this.detail = null
    this.waveform = null
    this.position = 0
    this.duration = track.duration
    this.audioError = null
    this.loading = true
    this.audio.src = audioUrl(track.id)
    if (autoplay) this.play()
    try {
      const { detail, waveform } = await loadDetail(track.id)
      if (token !== this.token) return
      this.detail = detail
      this.waveform = waveform
      if (!detail.fileExists) this.audioError = 'Audio file not found'
    } catch (e) {
      if (token === this.token) this.audioError = (e as Error).message
    } finally {
      if (token === this.token) this.loading = false
    }
  }

  play(): void {
    if (!this.track) return
    this.audio.play().catch(() => {})
  }

  toggle(): void {
    if (this.audio.paused) this.play()
    else this.audio.pause()
  }

  setMode(mode: WaveformMode): void {
    this.mode = mode
    localStorage.setItem('waveformMode', mode)
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(8, zoom))
    localStorage.setItem('waveformZoom', String(this.zoom))
  }
}
