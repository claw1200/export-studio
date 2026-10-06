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

  constructor() {
    this.audio.preload = 'auto'
    this.audio.addEventListener('play', () => {
      this.playing = true
      this.tick()
    })
    this.audio.addEventListener('pause', () => {
      this.playing = false
      cancelAnimationFrame(this.raf)
      this.position = this.audio.currentTime
    })
    this.audio.addEventListener('ended', () => (this.playing = false))
    this.audio.addEventListener('loadedmetadata', () => {
      if (Number.isFinite(this.audio.duration)) this.duration = this.audio.duration
    })
    this.audio.addEventListener('error', () => {
      const ext = this.track?.fileType ?? ''
      this.audioError =
        this.detail && !this.detail.fileExists
          ? 'Audio file not found'
          : `Preview not supported for ${ext || 'this file type'}`
    })
  }

  private tick = (): void => {
    this.position = this.audio.currentTime
    if (!this.audio.paused) this.raf = requestAnimationFrame(this.tick)
  }

  async load(track: Track, autoplay = false): Promise<void> {
    if (this.track?.id === track.id) {
      if (autoplay) this.play()
      return
    }
    const token = ++this.token
    this.audio.pause()
    this.track = track
    this.position = 0
    this.duration = track.duration
    this.audioError = null
    this.loading = true
    try {
      const { detail, waveform } = await loadDetail(track.id)
      if (token !== this.token) return
      this.detail = detail
      this.waveform = waveform
      if (waveform) this.duration = Math.max(track.duration, waveform.length / waveform.rate)
      this.audio.src = audioUrl(track.id)
      if (autoplay) this.play()
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

  seek(seconds: number): void {
    const t = Math.max(0, Math.min(seconds, this.duration || seconds))
    this.position = t
    this.audio.currentTime = t
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
