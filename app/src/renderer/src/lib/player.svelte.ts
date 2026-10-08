import { api } from './api'
import { bpmAt, type GridDraft, nearestBeat } from './beat'
import { CueEditor } from './deck/cues.svelte'
import { Deck, type DeckLoop } from './deck/deck.svelte'
import { GridEditor } from './deck/grid.svelte'
import { TrackSession } from './deck/session.svelte'
import { clampUnit, storedFlag, storedPercent, storedVolume, storeFlag } from './deck/stored'
import {
  beatLoop,
  CUE_TOLERANCE,
  type Hold,
  pressCue,
  pressHotCue,
  releaseHold,
  resizeLoop,
  type TransportState
} from './deck/transport'
import { WaveLoader, type WaveSamples } from './deck/waves.svelte'
import type { Beatgrid, Cue, Track, TrackDetail } from './types'
import type { Waveform, WaveformMode } from './waveform'

export { clearDetailCache, loadDetail } from './deck/session.svelte'

const CUE_HIT_TOLERANCE = 0.05
const FILE_MISSING = 'Audio file not found'

export type CuePlacement = 'playhead' | 'cuePoint'

/** Which button a hold belongs to: CUE, or a hot cue's slot. */
type HoldKey = 'cue' | number

/**
 * The deck as the interface sees it: transport and cue behaviour on top of the engine deck, the
 * track's library detail, cue and beatgrid editing, and the waveform display settings.
 */
export class Player {
  readonly session = new TrackSession()
  readonly deck = new Deck(0)
  readonly waves = new WaveLoader()
  readonly grid = new GridEditor(
    this.session,
    () => this.position,
    () => this.trackMs()
  )
  readonly cues = new CueEditor(this.session, this.grid)

  mode = $state<WaveformMode>((localStorage.getItem('waveformMode') as WaveformMode) || 'rgb')
  waveQuality = $state(storedPercent('waveQuality', 50))
  fallBlocks = $state(storedPercent('fallBlocks', 40))
  zoom = $state(Number(localStorage.getItem('waveformZoom')) || 1)
  volume = $state(storedVolume())
  muted = $state(storedFlag('muted'))
  quantise = $state(storedFlag('quantise'))
  cuePlacement = $state<CuePlacement>(localStorage.getItem('cuePlacement') === 'cuePoint' ? 'cuePoint' : 'playhead')
  /** Also the loop in point: LOOP OUT closes a loop from here. */
  cuePoint = $state(0)
  previewAt = $state<number | null>(null)
  private holding = $state.raw<(Hold & { key: HoldKey }) | null>(null)
  private previewOwner: string | null = null
  private unmuteVolume = 1

  constructor() {
    this.unmuteVolume = this.volume > 0 ? this.volume : 1
    this.deck.setVolume(this.muted ? 0 : this.volume)
    api.onEvent((event) => {
      if (event !== 'library.locked') return
      this.grid.cancel()
      void this.refreshDetail()
    })
  }

  get track(): Track | null {
    return this.session.track
  }

  set track(track: Track | null) {
    this.session.track = track
  }

  get detail(): TrackDetail | null {
    return this.session.detail
  }

  get waveform(): Waveform | null {
    return this.session.waveform
  }

  get wave(): WaveSamples | null {
    return this.waves.samples
  }

  get loading(): boolean {
    return this.session.loading
  }

  get editing(): boolean {
    return this.session.editing
  }

  get editError(): string | null {
    return this.session.editError
  }

  get playing(): boolean {
    return this.deck.playing
  }

  get position(): number {
    return this.deck.position
  }

  get duration(): number {
    return this.deck.duration
  }

  get loop(): DeckLoop | null {
    return this.deck.loop
  }

  get audioError(): string | null {
    if (this.detail && !this.detail.fileExists) return FILE_MISSING
    return this.session.error ?? this.deck.error
  }

  get cueHeld(): boolean {
    return !!this.holding
  }

  get gridEditing(): boolean {
    return this.grid.editing
  }

  get gridDraft(): GridDraft | null {
    return this.grid.draft
  }

  get gridDirty(): boolean {
    return this.grid.dirty
  }

  get gridLink(): boolean {
    return this.grid.link
  }

  get displayGrid(): Beatgrid | null {
    return this.grid.grid
  }

  get displayCues(): Cue[] {
    return this.grid.cues
  }

  get canEditGrid(): boolean {
    return this.grid.canEdit
  }

  get atCue(): boolean {
    return Math.abs(this.position - this.cuePoint) < CUE_TOLERANCE
  }

  private get state(): TransportState {
    return {
      position: this.position,
      playing: this.playing,
      scrubbing: this.deck.scrubbing,
      cuePoint: this.cuePoint
    }
  }

  async load(track: Track, autoplay = false): Promise<void> {
    if (this.track?.id === track.id) {
      if (autoplay) this.play()
      return
    }
    this.cuePoint = 0
    this.holding = null
    this.previewOwner = null
    this.previewAt = null
    this.grid.cancel()
    this.deck.load(track.id, track.duration)
    if (autoplay) this.deck.play()
    const opened = this.session.open(track)
    this.beginWave()
    await opened
  }

  play(): void {
    if (this.track) this.deck.play()
  }

  pause(): void {
    this.deck.pause()
  }

  /** Play while a cue is held makes the hold stick, as on a CDJ. */
  toggle(): void {
    if (this.holding) this.holding = { ...this.holding, latched: true }
    else if (this.playing) this.pause()
    else this.play()
  }

  /** Moves the playhead, leaving an active loop it lands outside of. */
  seek(seconds: number): void {
    const at = this.insideFile(seconds)
    this.leaveLoopOutside(at)
    if (this.deck.scrubbing) {
      this.deck.scrubTo(at)
      this.deck.scrubEnd()
    } else this.deck.seek(at)
  }

  /** Drags the playhead. `at` is the pointer event's timestamp, which the engine follows for speed. */
  scrub(seconds: number, at?: number): void {
    this.deck.scrubBegin()
    this.deck.scrubTo(this.insideFile(seconds), at)
  }

  endScrub(): void {
    if (!this.deck.scrubbing) return
    this.leaveLoopOutside(this.position)
    this.deck.scrubEnd()
  }

  private leaveLoopOutside(seconds: number): void {
    const loop = this.deck.loop
    if (loop?.active && (seconds < loop.start || seconds >= loop.end)) this.deck.setLooping(false)
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
    const end = this.duration
    const max = Number.isFinite(end) && end > 0.05 ? end - 0.001 : Math.max(0, end)
    return Math.max(0, Math.min(seconds, max || seconds))
  }

  private here(): number {
    return this.insideFile(this.quantised(this.position))
  }

  cueDown(): void {
    if (!this.track || this.holding || this.audioError) return
    const action = pressCue(this.state, this.here())
    switch (action.kind) {
      case 'mark':
        this.cuePoint = action.at
        break
      case 'return':
        this.deck.stopAt(action.at)
        break
      case 'set':
        this.cuePoint = action.at
        this.seek(action.at)
        break
      case 'hold':
        this.startHold('cue', action.at)
        break
    }
  }

  cueUp(): void {
    this.endHold('cue')
  }

  private startHold(key: HoldKey, at: number): void {
    this.holding = { key, at, latched: false }
    if (this.position !== at) this.seek(at)
    this.deck.play()
  }

  private endHold(key: HoldKey): void {
    const hold = this.holding
    if (hold?.key !== key) return
    this.holding = null
    const back = releaseHold(hold)
    if (back != null) this.deck.stopAt(back)
  }

  private hotCue(kind: number): Cue | undefined {
    return this.displayCues.find((c) => c.kind === 'hot' && c.slot === kind)
  }

  /** A set hot cue jumps while playing and plays while held when paused. An empty slot stores one. */
  hotCueDown(kind: number): void {
    const cue = this.hotCue(kind)
    if (!cue) {
      void this.setHotCue(kind)
      return
    }
    if (!this.track || this.holding || this.audioError) return
    const action = pressHotCue(this.state, { start: cue.start / 1000, end: cue.end == null ? null : cue.end / 1000 })
    if (action.loop) {
      this.cuePoint = action.loop[0]
      this.deck.setLoop(...action.loop)
    }
    if (action.kind === 'jump') {
      this.seek(action.at)
      return
    }
    this.cuePoint = action.at
    this.startHold(kind, action.at)
  }

  hotCueUp(kind: number): void {
    this.endHold(kind)
  }

  /** Calls a memory cue: it becomes the cue point, and a saved loop becomes the active loop. */
  callCue(cue: Cue): void {
    const start = this.insideFile(cue.start / 1000)
    this.cuePoint = start
    if (cue.end != null && cue.end > cue.start) this.deck.setLoop(start, cue.end / 1000)
    if (this.playing) this.seek(start)
    else this.deck.stopAt(start)
  }

  callMemoryCue(direction: -1 | 1): void {
    const memory = this.displayCues.filter((c) => c.kind === 'memory').sort((a, b) => a.start - b.start)
    const from = direction < 0 && this.playing ? this.cuePoint : this.position
    const cue =
      direction > 0
        ? memory.find((c) => c.start / 1000 > from + CUE_TOLERANCE)
        : memory.findLast((c) => c.start / 1000 < from - CUE_TOLERANCE)
    if (cue) this.callCue(cue)
  }

  /** LOOP IN: sets the cue point, which is the in point. */
  setLoopIn(): void {
    if (!this.track || this.audioError) return
    const here = this.here()
    this.cuePoint = here
    if (!this.playing && !this.deck.scrubbing) this.seek(here)
  }

  /** LOOP OUT: closes a loop from the cue point. */
  setLoopOut(): void {
    if (!this.track || this.audioError) return
    const start = this.cuePoint
    const end = this.here()
    if (!(end > start + 0.01)) return
    this.deck.setLoop(start, end)
  }

  /** RELOOP / EXIT. */
  reloopExit(): void {
    const loop = this.loop
    if (loop) this.deck.setLooping(!loop.active)
  }

  /** A loop of `beats` from the playhead, on the grid. Pressing it again on the same loop leaves it. */
  beatLoop(beats: number): void {
    if (!this.track || this.audioError) return
    const range = beatLoop(this.position, beats, this.displayGrid, this.track.bpm ?? 0, this.quantise)
    if (!range) return
    const loop = this.loop
    if (loop?.active && this.position >= loop.start && this.position < loop.end) {
      const same = Math.abs(loop.end - loop.start - (range[1] - range[0])) < 0.002
      if (same) {
        this.deck.setLooping(false)
        return
      }
      const resized = resizeLoop([loop.start, loop.end], (range[1] - range[0]) / (loop.end - loop.start))
      if (resized) this.deck.setLoop(...resized)
      return
    }
    this.cuePoint = range[0]
    this.deck.setLoop(range[0], Math.min(range[1], this.duration || range[1]))
  }

  /** Halves (0.5) or doubles (2) the loop from its in point. */
  resizeLoop(factor: number): void {
    const loop = this.loop
    if (!loop) return
    const resized = resizeLoop([loop.start, loop.end], factor)
    if (!resized) return
    const end = Math.min(resized[1], this.duration || resized[1])
    if (end > resized[0]) this.deck.setLoop(resized[0], end, loop.active)
  }

  async refreshDetail(): Promise<void> {
    await this.session.refresh()
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
    return this.cues.setHotCue(kind, this.placementMs())
  }

  addMemoryCue(): Promise<void> {
    return this.cues.addMemoryCue(this.placementMs())
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
    return this.cues.deleteCue(cue)
  }

  setCueColor(cue: Cue, colorIndex: number | null): Promise<void> {
    return this.cues.setCueColor(cue, colorIndex)
  }

  setQuantise(on: boolean): void {
    this.quantise = on
    storeFlag('quantise', on)
  }

  quantised(seconds: number): number {
    if (!this.quantise) return seconds
    return nearestBeat(seconds, this.displayGrid)
  }

  beginGridEdit(): void {
    this.grid.begin()
  }

  cancelGridEdit(): void {
    this.grid.cancel()
  }

  setGridLink(on: boolean): void {
    this.grid.setLink(on)
  }

  private trackMs(): number {
    const span = this.waveform ? this.waveform.length / this.waveform.rate : 0
    return Math.max(this.duration, span) * 1000
  }

  /** Shifts `base` (the current draft by default) by `ms`, so drags can work from their starting grid. */
  nudgeGrid(ms: number, base?: GridDraft | null): void {
    this.grid.nudge(ms, base === undefined ? this.grid.draft : base)
  }

  setDownbeatHere(): void {
    this.grid.setDownbeatHere()
  }

  /** Sets the tempo of `base` (the current draft by default), so drags can work from their starting grid. */
  setGridBpm(bpm: number, base?: GridDraft | null): void {
    this.grid.setBpm(bpm, base === undefined ? this.grid.draft : base)
  }

  halveGrid(): void {
    this.grid.halve()
  }

  doubleGrid(): void {
    this.grid.double()
  }

  saveGrid(): Promise<void> {
    return this.grid.save()
  }

  setMode(mode: WaveformMode): void {
    if (this.mode === mode) return
    this.mode = mode
    localStorage.setItem('waveformMode', mode)
    this.beginWave()
  }

  setWaveQuality(quality: number): void {
    const q = Math.min(100, Math.max(0, Math.round(quality)))
    if (this.waveQuality === q) return
    const wasOff = this.waveQuality <= 0
    this.waveQuality = q
    localStorage.setItem('waveQuality', String(q))
    if (q <= 0 || wasOff) this.beginWave()
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

  private beginWave(): void {
    const id = this.track?.id
    if (!id || this.waveQuality <= 0) this.waves.clear()
    else this.waves.load(id, this.mode === '3band')
  }

  setVolume(volume: number): void {
    const v = clampUnit(volume)
    this.volume = v
    if (v > 0) {
      this.unmuteVolume = v
      this.muted = false
    }
    this.deck.setVolume(this.muted ? 0 : v)
    localStorage.setItem('volume', String(v))
    storeFlag('muted', this.muted)
  }

  toggleMute(): void {
    if (this.muted || this.volume === 0) {
      this.muted = false
      this.setVolume(this.unmuteVolume > 0 ? this.unmuteVolume : 1)
      return
    }
    this.muted = true
    this.deck.setVolume(0)
    storeFlag('muted', true)
  }
}
