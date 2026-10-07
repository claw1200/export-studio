import { api, audioUrl } from './api'
import { bpmAt, draftOf, type GridDraft, nearestBeat, remapTime, retempoGrid, setDownbeat, shiftGrid } from './beat'
import { Lru } from './lru'
import { cachedWaveSamples, loadWaveSamples, type WaveSamples } from './peaks'
import type { Cue, CueMove, Track, TrackDetail } from './types'
import { decodeWaveform, type Waveform, type WaveformMode } from './waveform'

type DetailEntry = { detail: TrackDetail; waveform: Waveform | null }
const detailCache = new Lru<string, Promise<DetailEntry>>(24)
const SEEK_TOLERANCE = 0.05
const CUE_TOLERANCE = 0.001
const CUE_HIT_TOLERANCE = 0.05

export type CuePlacement = 'playhead' | 'cuePoint'
const FILE_MISSING = 'Audio file not found'

function storedVolume(): number {
  const raw = localStorage.getItem('volume')
  if (raw == null) return 1
  const n = Number(raw)
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

function storedUnit(key: string, fallback: number): number {
  const raw = localStorage.getItem(key)
  if (raw == null) return fallback
  const n = Number(raw)
  return Number.isFinite(n) ? Math.min(100, Math.max(0, Math.round(n))) : fallback
}

function storedWaveQuality(): number {
  return storedUnit('waveQuality', 50)
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
  waveQuality = $state(storedWaveQuality())
  fallBlocks = $state(storedUnit('fallBlocks', 40))
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)
  volume = $state(storedVolume())
  muted = $state(storedFlag('muted'))
  quantise = $state(storedFlag('quantise'))
  cuePlacement = $state<CuePlacement>(localStorage.getItem('cuePlacement') === 'cuePoint' ? 'cuePoint' : 'playhead')
  editing = $state(false)
  editError = $state<string | null>(null)
  cuePoint = $state(0)
  loopIn = $state<number | null>(null)
  cueHeld = $state(false)
  previewAt = $state<number | null>(null)
  gridEditing = $state(false)
  gridDraft = $state.raw<GridDraft | null>(null)
  gridDirty = $state(false)
  gridLink = $state(storedFlag('gridLink', true))
  readonly displayGrid = $derived(this.gridDraft ?? this.detail?.beatgrid ?? null)
  readonly displayCues = $derived.by(() => {
    const cues = this.detail?.cues ?? []
    const saved = this.detail?.beatgrid
    const draft = this.gridDraft
    if (!this.gridLink || !this.gridDirty || !saved || !draft) return cues
    const move = (ms: number): number => Math.max(0, Math.round(remapTime(ms, saved, draft)))
    return cues.map((cue) => ({ ...cue, start: move(cue.start), end: cue.end == null ? null : move(cue.end) }))
  })
  private previewOwner: string | null = null
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
      if (event !== 'library.locked') return
      this.cancelGridEdit()
      void this.refreshDetail()
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

  setPreview(owner: string, seconds: number): void {
    this.previewOwner = owner
    this.previewAt = seconds
  }

  clearPreview(owner: string): void {
    if (this.previewOwner !== owner) return
    this.previewOwner = null
    this.previewAt = null
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
    this.loopIn = null
    this.cueHeld = false
    this.cueLatched = false
    this.previewOwner = null
    this.previewAt = null
    this.cancelGridEdit()
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
    if (this.scrubState) {
      this.cuePoint = this.insideFile(this.quantised(this.position))
      return
    }
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

  setLoopIn(): void {
    if (!this.track || this.audioError) return
    const here = this.insideFile(this.quantised(this.position))
    this.loopIn = here
    this.cuePoint = here
    if (!this.playing && !this.scrubState) this.moveTo(here)
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

  setCuePlacement(placement: CuePlacement): void {
    this.cuePlacement = placement
    localStorage.setItem('cuePlacement', placement)
  }

  private placementMs(): number {
    const seconds = this.cuePlacement === 'cuePoint' ? this.cuePoint : this.position
    return Math.round(this.quantised(seconds) * 1000)
  }

  setHotCue(kind: number): Promise<void> {
    const ms = this.placementMs()
    return this.editCues((id) => api.setHotCue(id, kind, ms))
  }

  triggerHotCue(kind: number): void {
    const cue = this.displayCues.find((c) => c.kind === 'hot' && c.slot === kind)
    if (cue) this.seek(cue.start / 1000)
    else void this.setHotCue(kind)
  }

  callMemoryCue(direction: -1 | 1): void {
    const starts = this.displayCues
      .filter((c) => c.kind === 'memory')
      .map((c) => c.start / 1000)
      .sort((a, b) => a - b)
    const from = direction < 0 && this.playing ? this.cuePoint : this.position
    const target =
      direction > 0 ? starts.find((s) => s > from + CUE_TOLERANCE) : starts.findLast((s) => s < from - CUE_TOLERANCE)
    if (target == null) return
    this.cuePoint = this.insideFile(target)
    this.seek(target)
  }

  addMemoryCue(): Promise<void> {
    const ms = this.placementMs()
    return this.editCues((id) => api.addMemoryCue(id, ms))
  }

  cueAtPlayhead(): Cue | null {
    const tolerance = this.cueHitTolerance()
    let nearest: Cue | null = null
    let nearestGap = Infinity
    for (const cue of this.displayCues) {
      const gap = Math.abs(cue.start / 1000 - this.position)
      if (gap <= tolerance && gap < nearestGap) {
        nearest = cue
        nearestGap = gap
      }
    }
    return nearest
  }

  deleteCueAtPlayhead(): Promise<void> {
    const cue = this.cueAtPlayhead()
    return cue ? this.deleteCue(cue) : Promise.resolve()
  }

  private cueHitTolerance(): number {
    if (!this.quantise) return CUE_HIT_TOLERANCE
    const bpm = bpmAt(this.position, this.displayGrid, this.track?.bpm ?? 0)
    return bpm > 0 ? 30 / bpm : CUE_HIT_TOLERANCE
  }

  deleteCue(cue: Cue): Promise<void> {
    return this.editCues((id) => api.deleteCue(id, cue.id))
  }

  setCueColor(cue: Cue, colorIndex: number | null): Promise<void> {
    return this.editCues((id) => api.setCueColor(id, cue.id, colorIndex))
  }

  private async editCues(write: (id: string) => Promise<Cue[]>): Promise<void> {
    const id = this.track?.id
    if (!id || !this.detail || this.editing || this.gridEditing) return
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
    return nearestBeat(seconds, this.displayGrid)
  }

  get canEditGrid(): boolean {
    return !!this.detail?.beatgrid?.times.length
  }

  beginGridEdit(): void {
    const grid = this.detail?.beatgrid
    if (!grid?.times.length || this.gridEditing) return
    this.gridDraft = draftOf(grid)
    this.gridDirty = false
    this.gridEditing = true
    this.editError = null
  }

  cancelGridEdit(): void {
    this.gridEditing = false
    this.gridDraft = null
    this.gridDirty = false
  }

  setGridLink(on: boolean): void {
    this.gridLink = on
    storeFlag('gridLink', on)
  }

  private trackMs(): number {
    const span = this.waveform ? this.waveform.length / this.waveform.rate : 0
    return Math.max(this.duration, span) * 1000
  }

  private updateDraft(draft: GridDraft): void {
    this.gridDraft = draft
    this.gridDirty = true
  }

  /** Shifts `base` (the current draft by default) by `ms`, so drags can work from their starting grid. */
  nudgeGrid(ms: number, base: GridDraft | null = this.gridDraft): void {
    if (!this.gridEditing || !base) return
    this.updateDraft(shiftGrid(base, ms, this.trackMs()))
  }

  setDownbeatHere(): void {
    const draft = this.gridDraft
    if (!this.gridEditing || !draft) return
    this.updateDraft(setDownbeat(draft, this.position * 1000, this.trackMs()))
  }

  private retempo(bpm: (current: number) => number, density = 1, draft = this.gridDraft): void {
    if (!this.gridEditing || !draft) return
    const current = bpmAt(this.position, draft)
    if (!(current > 0)) return
    this.updateDraft(retempoGrid(draft, bpm(current), this.position * 1000, this.trackMs(), density))
  }

  /** Sets the tempo of `base` (the current draft by default), so drags can work from their starting grid. */
  setGridBpm(bpm: number, base: GridDraft | null = this.gridDraft): void {
    if (Number.isFinite(bpm) && bpm > 0) this.retempo(() => bpm, 1, base)
  }

  adjustGridBpm(delta: number): void {
    this.retempo((current) => current + delta)
  }

  halveGrid(): void {
    this.retempo((current) => current / 2, 0.5)
  }

  doubleGrid(): void {
    this.retempo((current) => current * 2, 2)
  }

  private movedCues(): CueMove[] {
    if (!this.gridLink) return []
    const before = new Map((this.detail?.cues ?? []).map((c) => [c.id, c]))
    return this.displayCues
      .filter((c) => {
        const was = before.get(c.id)
        return was && (was.start !== c.start || was.end !== c.end)
      })
      .map((c) => ({ id: c.id, start: c.start, end: c.end }))
  }

  async saveGrid(): Promise<void> {
    const id = this.track?.id
    const draft = this.gridDraft
    if (!id || !this.detail || !draft || this.editing) return
    if (!this.gridDirty) {
      this.cancelGridEdit()
      return
    }
    const grid = { times: draft.times.map((t) => Math.round(t)), beats: draft.beats, bpms: draft.bpms }
    const moves = this.movedCues()
    this.editing = true
    this.editError = null
    try {
      const saved = await api.setBeatgrid(id, grid, moves)
      if (this.track?.id !== id || !this.detail) {
        detailCache.delete(id)
        return
      }
      this.detail = { ...this.detail, beatgrid: saved.beatgrid, cues: saved.cues }
      if (saved.bpm != null && this.track) this.track = { ...this.track, bpm: saved.bpm }
      detailCache.set(id, Promise.resolve({ detail: this.detail, waveform: this.waveform }))
      this.cancelGridEdit()
    } catch (e) {
      if (this.track?.id === id) this.editError = (e as Error).message
    } finally {
      this.editing = false
    }
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    localStorage.setItem('waveformMode', mode)
    if (this.track) this.beginWave(this.token, this.track.id)
  }

  setWaveQuality(quality: number): void {
    const q = Math.min(100, Math.max(0, Math.round(quality)))
    if (this.waveQuality === q) return
    const wasOff = this.waveQuality <= 0
    this.waveQuality = q
    localStorage.setItem('waveQuality', String(q))
    if (!this.track) {
      if (q <= 0) {
        this.waveAbort?.abort()
        this.waveAbort = null
        this.wave = null
      }
      return
    }
    if (q <= 0 || wasOff) this.beginWave(this.token, this.track.id)
  }

  setFallBlocks(blocks: number): void {
    const n = Math.min(100, Math.max(0, Math.round(blocks)))
    if (this.fallBlocks === n) return
    this.fallBlocks = n
    localStorage.setItem('fallBlocks', String(n))
  }

  setZoom(zoom: number): void {
    this.zoom = Math.max(0.25, Math.min(64, zoom))
    localStorage.setItem('waveformZoom', String(this.zoom))
  }

  private beginWave(token: number, id: string): void {
    this.waveAbort?.abort()
    if (this.waveQuality <= 0) {
      this.waveAbort = null
      this.wave = null
      return
    }
    const ctrl = new AbortController()
    this.waveAbort = ctrl
    const url = audioUrl(id)
    const wantBands = this.mode === '3band'
    const cached = cachedWaveSamples(url)
    this.wave = cached
    if (cached && (!wantBands || cached.bands)) return
    void loadWaveSamples(url, ctrl.signal, wantBands).then((samples) => {
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
