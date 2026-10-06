import { api, audioUrl } from './api'
import { nearestBeat } from './beat'
import { Lru } from './lru'
import { cachedWaveSamples, loadWaveSamples, type WaveSamples } from './peaks'
import type { Cue, Track, TrackDetail } from './types'
import { decodeWaveform, type Waveform, type WaveformMode } from './waveform'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Lru<string, Promise<DetailEntry>>(24)
const SEEK_TOLERANCE = 0.05
const FILE_MISSING = 'Audio file not found'

function storedVolume(): number {
  const n = Number(localStorage.getItem('volume'))
  return Number.isFinite(n) ? clampUnit(n) : 1
}

function clampUnit(n: number): number {
  return Math.min(1, Math.max(0, n))
}

function storedFlag(key: string, fallback = false): boolean {
  const value = localStorage.getItem(key)
  if (value == null) return fallback
  return value === '1'
}

function storeFlag(key: string, on: boolean): void {
  localStorage.setItem(key, on ? '1' : '0')
}

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

export class Player {
  track = $state.raw<Track | null>(null)
  detail = $state.raw<TrackDetail | null>(null)
  waveform = $state.raw<Waveform | null>(null)
  wave = $state.raw<WaveSamples | null>(null)
  loading = $state(false)
  playing = $state(false)
  position = $state(0)
  duration = $state(0)
  audioError = $state<string | null>(null)
  mode = $state<WaveformMode>((localStorage.getItem('waveformMode') as WaveformMode) || 'rgb')
  hqRgb = $state(localStorage.getItem('hqRgb') !== '0')
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)
  volume = $state(storedVolume())
  muted = $state(storedFlag('muted'))
  quantise = $state(storedFlag('quantise'))
  editing = $state(false)
  editError = $state<string | null>(null)
  private unmuteVolume = 1

  private audio = new Audio()
  private raf = 0
  private token = 0
  private scrubbing = false
  private seekTarget: number | null = null
  private seekBeforeMetadata: number | null = null
  private playheadBeforePlay: number | null = null
  private waveAbort: AbortController | null = null

  constructor() {
    this.unmuteVolume = this.volume > 0 ? this.volume : 1
    this.audio.volume = this.volume
    this.audio.muted = this.muted
    this.audio.preload = 'auto'
    this.audio.addEventListener('play', () => {
      this.playing = true
      this.tick()
    })
    this.audio.addEventListener('playing', () => {
      const playhead = this.playheadBeforePlay
      this.playheadBeforePlay = null
      if (playhead == null || this.scrubbing) return
      if (this.near(playhead)) return
      this.position = playhead
      this.seekTarget = null
      this.apply(playhead)
    })
    this.audio.addEventListener('pause', () => {
      this.playing = false
      cancelAnimationFrame(this.raf)
      if (!this.scrubbing && this.seekTarget == null && this.playheadBeforePlay == null)
        this.position = this.audio.currentTime
    })
    this.audio.addEventListener('ended', () => (this.playing = false))
    this.audio.addEventListener('seeked', () => {
      const sent = this.seekTarget
      this.seekTarget = null
      if (sent != null && Math.abs(this.position - sent) >= SEEK_TOLERANCE) this.apply(this.position)
    })
    this.audio.addEventListener('loadedmetadata', () => {
      if (Number.isFinite(this.audio.duration)) this.duration = this.audio.duration
      if (this.seekBeforeMetadata != null && !this.scrubbing) {
        const t = this.seekBeforeMetadata
        this.seekBeforeMetadata = null
        this.apply(t)
      }
    })
    api.onEvent((event) => {
      if (event === 'library.locked') void this.refreshDetail()
    })
    this.audio.addEventListener('error', () => {
      if (this.sourceWasReplaced()) return
      const ext = this.track?.fileType ?? ''
      this.audioError =
        this.detail && !this.detail.fileExists ? FILE_MISSING : `Preview not supported for ${ext || 'this file type'}`
    })
  }

  private near(seconds: number): boolean {
    return Math.abs(this.audio.currentTime - seconds) < SEEK_TOLERANCE
  }

  private sourceWasReplaced(): boolean {
    return this.audio.error?.code === MediaError.MEDIA_ERR_ABORTED
  }

  private tick = (): void => {
    if (!this.scrubbing && this.seekTarget == null && this.playheadBeforePlay == null)
      this.position = this.audio.currentTime
    if (!this.audio.paused) this.raf = requestAnimationFrame(this.tick)
  }

  scrub(seconds: number): void {
    this.scrubbing = true
    this.apply(seconds)
  }

  endScrub(): void {
    if (!this.scrubbing) return
    this.scrubbing = false
    this.apply(this.position)
  }

  seek(seconds: number): void {
    this.scrubbing = false
    this.apply(seconds)
  }

  private insideFile(seconds: number): number {
    const end = Number.isFinite(this.audio.duration) && this.audio.duration > 0 ? this.audio.duration : this.duration
    const max = Number.isFinite(end) && end > 0.05 ? end - 0.001 : Math.max(0, end)
    return Math.max(0, Math.min(seconds, max || seconds))
  }

  private apply(seconds: number): void {
    const t = this.insideFile(seconds)
    this.position = t
    if (this.audio.readyState < HTMLMediaElement.HAVE_METADATA) {
      this.seekBeforeMetadata = t
      return
    }
    this.seekBeforeMetadata = null
    if (this.seekTarget != null || this.near(t)) return
    this.seekTarget = t
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
    this.seekTarget = null
    this.seekBeforeMetadata = null
    this.playheadBeforePlay = null
    this.track = track
    this.detail = null
    this.waveform = null
    this.beginWave(token, track.id)
    this.position = 0
    this.duration = track.duration
    this.audioError = null
    this.editError = null
    this.loading = true
    this.audio.src = audioUrl(track.id)
    if (autoplay) this.play()
    try {
      const { detail, waveform } = await loadDetail(track.id)
      if (token !== this.token) return
      this.detail = detail
      this.waveform = waveform
      if (!detail.fileExists) this.audioError = FILE_MISSING
    } catch (e) {
      if (token === this.token) this.audioError = (e as Error).message
    } finally {
      if (token === this.token) this.loading = false
    }
  }

  play(): void {
    if (!this.track) return
    this.playheadBeforePlay = this.position
    if (!this.near(this.position)) {
      this.seekTarget = null
      this.apply(this.position)
    }
    void this.audio.play().catch(() => {
      this.playheadBeforePlay = null
    })
  }

  toggle(): void {
    if (this.audio.paused) this.play()
    else this.audio.pause()
  }

  async refreshDetail(): Promise<void> {
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

  setHotCue(kind: number): Promise<void> {
    const ms = Math.round(this.quantised(this.position) * 1000)
    return this.editCues((id) => api.setHotCue(id, kind, ms))
  }

  deleteCue(cue: Cue): Promise<void> {
    return this.editCues((id) => api.deleteCue(id, cue.id))
  }

  setCueColor(cue: Cue, colorIndex: number | null): Promise<void> {
    return this.editCues((id) => api.setCueColor(id, cue.id, colorIndex))
  }

  private async editCues(write: (id: string) => Promise<Cue[]>): Promise<void> {
    const id = this.track?.id
    if (!id || !this.detail || this.editing) return
    this.editing = true
    this.editError = null
    try {
      const cues = await write(id)
      if (this.track?.id !== id || !this.detail) {
        detailCache.delete(id)
        return
      }
      this.detail = { ...this.detail, cues }
      detailCache.set(id, Promise.resolve({ detail: this.detail, waveform: this.waveform }))
    } catch (e) {
      if (this.track?.id === id) this.editError = (e as Error).message
    } finally {
      this.editing = false
    }
  }

  setQuantise(on: boolean): void {
    this.quantise = on
    storeFlag('quantise', on)
  }

  quantised(seconds: number): number {
    if (!this.quantise) return seconds
    return nearestBeat(seconds, this.detail?.beatgrid ?? null)
  }

  setMode(mode: WaveformMode): void {
    this.mode = mode
    localStorage.setItem('waveformMode', mode)
  }

  setHqRgb(on: boolean): void {
    if (this.hqRgb === on) return
    this.hqRgb = on
    storeFlag('hqRgb', on)
    if (this.track) this.beginWave(this.token, this.track.id)
    else if (!on) {
      this.waveAbort?.abort()
      this.waveAbort = null
      this.wave = null
    }
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    localStorage.setItem('waveformZoom', String(this.zoom))
  }

  private beginWave(token: number, id: string): void {
    this.waveAbort?.abort()
    if (!this.hqRgb) {
      this.waveAbort = null
      this.wave = null
      return
    }
    const ctrl = new AbortController()
    this.waveAbort = ctrl
    const url = audioUrl(id)
    const cached = cachedWaveSamples(url)
    this.wave = cached
    if (cached) return
    void loadWaveSamples(url, ctrl.signal).then((samples) => {
      if (token === this.token && this.waveAbort === ctrl) this.wave = samples
    })
  }

  setVolume(volume: number): void {
    const v = clampUnit(volume)
    this.volume = v
    this.audio.volume = v
    if (v > 0) {
      this.unmuteVolume = v
      this.muted = false
      this.audio.muted = false
    }
    localStorage.setItem('volume', String(v))
    storeFlag('muted', this.muted)
  }

  toggleMute(): void {
    if (this.muted || this.volume === 0) {
      this.muted = false
      this.audio.muted = false
      this.setVolume(this.unmuteVolume > 0 ? this.unmuteVolume : 1)
      return
    }
    this.muted = true
    this.audio.muted = true
    storeFlag('muted', true)
  }
}
