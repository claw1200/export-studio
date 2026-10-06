import { api, audioUrl } from './api'
import { cachedWaveSamples, loadWaveSamples, type WaveSamples } from './peaks'
import type { Track, TrackDetail } from './types'
import { decodeWaveform, type Waveform, type WaveformMode } from './waveform'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Map<string, Promise<DetailEntry>>()
const CACHE_SIZE = 24

function storedVolume(): number {
  const n = Number(localStorage.getItem('volume'))
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 1
}

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
  /** PCM for the RGB detail view. Null until the audio has been decoded. */
  wave = $state.raw<WaveSamples | null>(null)
  loading = $state(false)
  playing = $state(false)
  position = $state(0)
  duration = $state(0)
  audioError = $state<string | null>(null)
  mode = $state<WaveformMode>((localStorage.getItem('waveformMode') as WaveformMode) || 'rgb')
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)
  volume = $state(storedVolume())
  muted = $state(localStorage.getItem('muted') === '1')
  /** Level restored by unmute. */
  private level = 1

  private audio = new Audio()
  private raf = 0
  private token = 0
  /** Playhead is following a gesture, so playback must not overwrite it. */
  private scrubbing = false
  /** Target of the seek the element is currently performing. */
  private sent: number | null = null
  /** Seek made before the element knew its duration. */
  private pending: number | null = null
  /** Playhead to restore if starting playback resets the element to 0. */
  private resume: number | null = null
  private waveAbort: AbortController | null = null

  constructor() {
    this.level = this.volume > 0 ? this.volume : 1
    this.audio.volume = this.volume
    this.audio.muted = this.muted
    this.audio.preload = 'auto'
    this.audio.addEventListener('play', () => {
      this.playing = true
      this.tick()
    })
    this.audio.addEventListener('playing', () => {
      const resume = this.resume
      this.resume = null
      if (resume == null || this.scrubbing) return
      if (Math.abs(this.audio.currentTime - resume) < 0.05) return
      this.position = resume
      this.sent = null
      this.apply(resume)
    })
    this.audio.addEventListener('pause', () => {
      this.playing = false
      cancelAnimationFrame(this.raf)
      if (!this.scrubbing && this.sent == null && this.resume == null) this.position = this.audio.currentTime
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
    if (!this.scrubbing && this.sent == null && this.resume == null) this.position = this.audio.currentTime
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

  /** The sample at `duration` is past the end of the file, so stay just inside it. */
  private playable(seconds: number): number {
    const end = Number.isFinite(this.audio.duration) && this.audio.duration > 0 ? this.audio.duration : this.duration
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
    this.resume = null
    this.track = track
    this.detail = null
    this.waveform = null
    this.beginWave(token, track.id)
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
    this.resume = this.position
    if (Math.abs(this.audio.currentTime - this.position) >= 0.05) {
      this.sent = null
      this.apply(this.position)
    }
    void this.audio.play().catch(() => {
      this.resume = null
    })
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
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    localStorage.setItem('waveformZoom', String(this.zoom))
  }

  /** Starts decoding PCM for the RGB wave. Cached tracks show up immediately. */
  private beginWave(token: number, id: string): void {
    this.waveAbort?.abort()
    const ctrl = new AbortController()
    this.waveAbort = ctrl
    const url = audioUrl(id)
    const cached = cachedWaveSamples(url)
    this.wave = cached
    if (cached) return
    void loadWaveSamples(url, ctrl.signal).then((samples) => {
      if (token === this.token) this.wave = samples
    })
  }

  setVolume(volume: number): void {
    const v = Math.min(1, Math.max(0, volume))
    this.volume = v
    this.audio.volume = v
    if (v > 0) {
      this.level = v
      this.muted = false
      this.audio.muted = false
    }
    localStorage.setItem('volume', String(v))
    localStorage.setItem('muted', this.muted ? '1' : '0')
  }

  toggleMute(): void {
    if (this.muted || this.volume === 0) {
      this.muted = false
      this.audio.muted = false
      this.setVolume(this.level > 0 ? this.level : 1)
      return
    }
    this.muted = true
    this.audio.muted = true
    localStorage.setItem('muted', '1')
  }
}
