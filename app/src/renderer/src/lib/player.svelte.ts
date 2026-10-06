import { api, audioUrl } from './api'
import { nearestBeat } from './beat'
import { Lru } from './lru'
import { cachedWaveSamples, loadWaveSamples, type WaveSamples } from './peaks'
import type { Cue, Track, TrackDetail } from './types'
import { decodeWaveform, type Waveform, type WaveformMode } from './waveform'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Lru<string, Promise<DetailEntry>>(24)
const SEEK_TOLERANCE = 0.05
const CUE_TOLERANCE = 0.001
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
  cuePoint = $state(0)
  cueHeld = $state(false)
  private cueLatched = false
  private unmuteVolume = 1

  private audio = new Audio()
  private raf = 0
  private token = 0
  private scrubState: { resume: boolean; parked: boolean } | null = null
  private holdTimer = 0
  private sent: number | null = null
  private deferred = false
  private starting = false
  private waveAbort: AbortController | null = null

  constructor() {
    this.unmuteVolume = this.volume > 0 ? this.volume : 1
    this.audio.volume = this.volume
    this.audio.muted = this.muted
    this.audio.preload = 'auto'
    this.audio.addEventListener('play', () => {
      if (this.audio.paused) return
      this.playing = true
      cancelAnimationFrame(this.raf)
      this.tick()
    })
    this.audio.addEventListener('playing', () => {
      if (!this.starting) return
      this.starting = false
      if (!this.scrubState && this.sent == null && !this.deferred && !this.near(this.position)) this.syncElement()
    })
    this.audio.addEventListener('pause', () => {
      cancelAnimationFrame(this.raf)
      if (!this.playing || this.scrubState?.parked) return
      this.playing = false
      if (this.elementOwnsPosition) this.position = this.audio.currentTime
    })
    this.audio.addEventListener('ended', () => (this.playing = false))
    this.audio.addEventListener('seeked', () => {
      const sent = this.sent
      this.sent = null
      if (sent != null && sent !== this.position) this.syncElement()
    })
    this.audio.addEventListener('loadedmetadata', () => {
      if (Number.isFinite(this.audio.duration)) this.duration = this.audio.duration
      if (this.deferred) this.syncElement()
    })
    api.onEvent((event) => {
      if (event === 'library.locked') void this.refreshDetail()
    })
    this.audio.addEventListener('error', () => {
      if (this.sourceWasReplaced()) return
      this.sent = null
      this.deferred = false
      this.starting = false
      this.playing = false
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

  private get elementOwnsPosition(): boolean {
    return !this.scrubState && this.sent == null && !this.deferred && !this.starting
  }

  private tick = (): void => {
    if (this.elementOwnsPosition) this.position = this.audio.currentTime
    if (!this.audio.paused) this.raf = requestAnimationFrame(this.tick)
  }

  scrub(seconds: number): void {
    window.clearTimeout(this.holdTimer)
    const moved = !!this.scrubState && this.insideFile(seconds) !== this.position
    const scrub = (this.scrubState ??= { resume: this.playing, parked: false })
    this.moveTo(seconds)
    if (!scrub.resume) return
    if (moved && scrub.parked) {
      scrub.parked = false
      void this.audio.play().then(
        () => {
          if (this.scrubState?.parked) this.audio.pause()
        },
        () => {}
      )
    }
    this.holdTimer = window.setTimeout(() => this.holdStill(), 120)
  }

  private holdStill(): void {
    const scrub = this.scrubState
    if (!scrub) return
    scrub.parked = true
    if (!this.audio.paused) this.audio.pause()
    this.syncElement()
  }

  endScrub(): void {
    window.clearTimeout(this.holdTimer)
    const scrub = this.scrubState
    if (!scrub) return
    this.scrubState = null
    this.syncElement()
    if (scrub.resume && this.audio.paused) this.play()
  }

  seek(seconds: number): void {
    this.moveTo(seconds)
    this.endScrub()
  }

  private insideFile(seconds: number): number {
    const end = Number.isFinite(this.audio.duration) && this.audio.duration > 0 ? this.audio.duration : this.duration
    const max = Number.isFinite(end) && end > 0.05 ? end - 0.001 : Math.max(0, end)
    return Math.max(0, Math.min(seconds, max || seconds))
  }

  private moveTo(seconds: number): void {
    this.position = this.insideFile(seconds)
    this.syncElement()
  }

  private syncElement(): void {
    if (this.audio.readyState < HTMLMediaElement.HAVE_METADATA) {
      this.deferred = true
      return
    }
    this.deferred = false
    if (this.sent != null) return
    this.sent = this.position
    this.audio.currentTime = this.position
  }

  private stopAt(seconds: number): void {
    window.clearTimeout(this.holdTimer)
    this.scrubState = null
    this.playing = false
    this.starting = false
    this.audio.pause()
    this.moveTo(seconds)
  }

  async load(track: Track, autoplay = false): Promise<void> {
    if (this.track?.id === track.id) {
      if (autoplay) this.play()
      return
    }
    const token = ++this.token
    window.clearTimeout(this.holdTimer)
    this.scrubState = null
    this.playing = false
    this.starting = false
    this.audio.pause()
    this.sent = null
    this.deferred = false
    this.cuePoint = 0
    this.cueHeld = false
    this.cueLatched = false
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
    this.playing = true
    if (!this.audio.paused) return
    this.starting = true
    const token = this.token
    void this.audio.play().catch(() => {
      if (token !== this.token || !this.audio.paused) return
      this.starting = false
      if (!this.scrubState) this.playing = false
    })
  }

  pause(): void {
    if (this.scrubState) this.scrubState.resume = false
    const owned = this.elementOwnsPosition
    this.playing = false
    this.starting = false
    this.audio.pause()
    if (owned) this.position = this.audio.currentTime
  }

  toggle(): void {
    if (this.cueHeld) this.cueLatched = true
    else if (this.playing) this.pause()
    else this.play()
  }

  get atCue(): boolean {
    return Math.abs(this.position - this.cuePoint) < CUE_TOLERANCE
  }

  cueDown(): void {
    if (!this.track || this.cueHeld || this.audioError) return
    if (this.playing) {
      this.stopAt(this.cuePoint)
      return
    }
    const here = this.insideFile(this.quantised(this.position))
    if (Math.abs(here - this.cuePoint) >= CUE_TOLERANCE) {
      this.cuePoint = here
      this.moveTo(here)
      return
    }
    this.cueHeld = true
    this.cueLatched = false
    if (this.position !== this.cuePoint) this.moveTo(this.cuePoint)
    this.play()
  }

  cueUp(): void {
    if (!this.cueHeld) return
    this.cueHeld = false
    if (this.cueLatched) {
      this.cueLatched = false
      return
    }
    this.stopAt(this.cuePoint)
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
