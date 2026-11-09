import { api } from './api'
import { bpmAt, nearestBeat } from './beat'
import type { Seconds } from './deck/clock'
import { CueEditor } from './deck/cues.svelte'
import { Deck, type DeckLoop } from './deck/deck.svelte'
import { GridEditor } from './deck/grid.svelte'
import { TrackSession } from './deck/session.svelte'
import { WaveformDisplay } from './deck/display.svelte'
import type { CuePlacement, DeckPrefs } from './deck/prefs.svelte'
import { TempoControl } from './deck/tempo.svelte'
import {
  beatLoop,
  CUE_TOLERANCE,
  cueRange,
  cueToStore,
  type Hold,
  loopOf,
  pressCue,
  pressHotCue,
  releaseHold,
  resizeLoop,
  SHORTEST_LOOP,
  type TransportState
} from './deck/transport'
import type { Beatgrid, Cue, Track, TrackDetail } from './types'
import type { Waveform } from './waveform/data'

const CUE_HIT_TOLERANCE: Seconds = 0.05
const END_MARGIN: Seconds = 0.001
const SAME_LOOP_LENGTH_TOLERANCE: Seconds = 0.002
/** Rekordbox's detail rate, so zoom covers the same time for a file that has no analysis. */
const LOOSE_WAVE_RATE = 150
/** Matches the error the audio bridge sends when a track's file can't be resolved. */
const FILE_MISSING = 'Audio file not found'
const STREAMING = 'Streaming tracks can’t be played here'

type HotCueKind = number
type HoldButton = 'cue' | HotCueKind

/** The deck as the interface sees it: CDJ transport, cue and loop behaviour on top of the engine deck. */
export class Player {
  private readonly session = new TrackSession()
  onTakeSpeakers: (() => void) | null = null
  private pausedForPreview = false
  readonly deck: Deck
  readonly grid = new GridEditor(
    this.session,
    () => this.position,
    () => this.trackMs(),
    (seconds) => this.seek(seconds)
  )
  readonly cues = new CueEditor(this.session, this.grid)
  readonly tempo: TempoControl
  readonly display: WaveformDisplay

  /** Also the loop in point: LOOP OUT closes a loop from here. */
  cuePoint = $state<Seconds>(0)
  previewAt = $state<Seconds | null>(null)
  private holding = $state.raw<(Hold & { key: HoldButton }) | null>(null)
  private previewOwner: string | null = null
  private fileListeners = new Set<(id: string, missing: boolean) => void>()

  constructor(
    engineDeck: number,
    private readonly prefs: DeckPrefs
  ) {
    this.deck = new Deck(engineDeck, prefs)
    this.tempo = new TempoControl(this.deck)
    this.display = new WaveformDisplay(
      prefs.waveform,
      () => this.track?.id ?? null,
      () => this.track?.collected === false
    )
    this.deck.onLoadSettled = () => {
      const id = this.track?.id
      const missing = this.fileMissing
      if (id && missing !== null) for (const listener of this.fileListeners) listener(id, missing)
    }
    api.onEvent((event) => {
      if (event !== 'library.locked') return
      this.grid.cancel()
      void this.refreshDetail()
    })
  }

  get quantise(): boolean {
    return this.prefs.quantise
  }

  get cuePlacement(): CuePlacement {
    return this.prefs.cuePlacement
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
    if (this.session.waveform) return this.session.waveform
    if (this.track?.collected !== false) return null
    const samples = this.display.samples
    if (!samples || samples.rate <= 0 || samples.data.length < 2) return null
    const duration = samples.data.length / samples.rate
    return { rate: LOOSE_WAVE_RATE, length: Math.max(2, duration * LOOSE_WAVE_RATE) }
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

  get position(): Seconds {
    return this.deck.position
  }

  get duration(): Seconds {
    return this.deck.duration
  }

  get loop(): DeckLoop | null {
    return this.deck.loop
  }

  private get fileMissing(): boolean | null {
    const track = this.track
    if (!track) return null
    if (track.streaming) return false
    if (this.deck.error === FILE_MISSING) return true
    return this.deck.ready ? false : null
  }

  onFileChecked(listener: (id: string, missing: boolean) => void): () => void {
    this.fileListeners.add(listener)
    return () => this.fileListeners.delete(listener)
  }

  get playable(): boolean {
    return !!this.track && !this.audioError
  }

  get audioError(): string | null {
    if (this.track?.streaming) return STREAMING
    if (this.fileMissing) return FILE_MISSING
    return this.session.error ?? this.deck.error
  }

  get cueHeld(): boolean {
    return !!this.holding
  }

  get displayGrid(): Beatgrid | null {
    return this.grid.grid
  }

  get trackBpmAtPlayhead(): number {
    return bpmAt(this.position, this.displayGrid, this.track?.bpm ?? 0)
  }

  get displayCues(): Cue[] {
    return this.grid.cues
  }

  get atCue(): boolean {
    return Math.abs(this.position - this.cuePoint) < CUE_TOLERANCE
  }

  private get canCue(): boolean {
    return this.playable && !this.holding
  }

  private get state(): TransportState {
    return {
      position: this.position,
      playing: this.playing,
      scrubbing: this.deck.scrubbing,
      cuePoint: this.cuePoint
    }
  }

  async load(track: Track): Promise<void> {
    if (!this.alreadyLoaded(track)) await this.open(track)
  }

  async loadAndPlay(track: Track): Promise<void> {
    if (this.alreadyLoaded(track)) this.play()
    else await this.open(track, { play: true })
  }

  async reopen(track: Track): Promise<void> {
    if (this.track?.id === track.id) await this.open(track)
  }

  private alreadyLoaded(track: Track): boolean {
    return this.track?.id === track.id && !this.fileMissing
  }

  private resetForNewTrack(): void {
    this.cuePoint = 0
    this.holding = null
    this.previewOwner = null
    this.previewAt = null
    this.grid.cancel()
  }

  eject(): void {
    if (!this.track) return
    this.resetForNewTrack()
    this.deck.unload()
    this.session.close()
    this.display.reload()
  }

  private async open(track: Track, { play = false } = {}): Promise<void> {
    this.resetForNewTrack()
    if (play) this.takeSpeakers()
    this.deck.load(track.id, track.duration)
    if (play) this.deck.play()
    const opened = this.session.open(track)
    this.display.reload()
    await opened
  }

  private play(): void {
    if (!this.track) return
    this.takeSpeakers()
    this.deck.play()
  }

  yieldToPreview(): void {
    if (!this.playing || this.pausedForPreview) return
    this.pausedForPreview = true
    this.pause()
  }

  static resumeAfterPreview(players: readonly Player[]): void {
    const paused = players.filter((player) => player.pausedForPreview && player.track)
    for (const player of players) player.pausedForPreview = false
    Deck.playTogether(paused.map((player) => player.deck))
  }

  static playTogether(players: readonly Player[]): void {
    const playable = players.filter((player) => player.playable)
    for (const player of playable) player.takeSpeakers()
    Deck.playTogether(playable.map((player) => player.deck))
  }

  stopAt(seconds: Seconds): void {
    if (!this.track) return
    this.deck.stopAt(this.insideFile(seconds))
  }

  private takeSpeakers(): void {
    this.pausedForPreview = false
    this.onTakeSpeakers?.()
  }

  pause(): void {
    this.deck.pause()
  }

  toggle(): void {
    if (this.holding) this.latchHold()
    else if (this.playing) this.pause()
    else this.play()
  }

  private latchHold(): void {
    if (!this.holding) return
    this.holding = { ...this.holding, latched: true }
    this.deck.align()
  }

  seek(seconds: Seconds): void {
    const at = this.insideFile(seconds)
    this.leaveLoopOutside(at)
    if (this.deck.scrubbing) {
      this.deck.scrubTo(at)
      this.deck.scrubEnd()
    } else this.deck.seek(at)
  }

  scrub(seconds: Seconds, inputTimestamp?: number): void {
    this.deck.scrubBegin()
    this.deck.scrubTo(this.insideFile(seconds), inputTimestamp)
  }

  endScrub(): void {
    if (!this.deck.scrubbing) return
    this.leaveLoopOutside(this.position)
    this.deck.scrubEnd()
  }

  private leaveLoopOutside(seconds: Seconds): void {
    const loop = this.loop
    if (loop?.active && (seconds < loop.start || seconds >= loop.end)) this.deck.setLooping(false)
  }

  setPreview(owner: string, seconds: Seconds): void {
    this.previewOwner = owner
    this.previewAt = seconds
  }

  clearPreview(owner: string): void {
    if (this.previewOwner !== owner) return
    this.previewOwner = null
    this.previewAt = null
  }

  private insideFile(seconds: Seconds): Seconds {
    const end = this.duration
    const last = Number.isFinite(end) && end > 0 ? Math.max(0, end - END_MARGIN) : Infinity
    return Math.max(0, Math.min(seconds, last))
  }

  private notPastEnd(seconds: Seconds): Seconds {
    return Math.min(seconds, this.duration || seconds)
  }

  private quantisedPlayhead(): Seconds {
    return this.insideFile(this.quantised(this.position))
  }

  cueDown(): void {
    if (!this.canCue) return
    const action = pressCue(this.state, this.quantisedPlayhead())
    switch (action.kind) {
      case 'setCueOnly':
        this.cuePoint = action.at
        break
      case 'stopAtCue':
        this.deck.stopAt(action.at)
        break
      case 'setCueAndGo':
        this.cuePoint = action.at
        this.seek(action.at)
        break
      case 'holdFromCue':
        this.startHold('cue', action.at)
        break
    }
  }

  cueUp(): void {
    this.endHold('cue')
  }

  private startHold(key: HoldButton, at: Seconds): void {
    this.takeSpeakers()
    this.holding = { key, at, latched: false }
    if (this.position !== at) this.seek(at)
    this.deck.play({ onMasterBeat: false })
  }

  private endHold(key: HoldButton): void {
    const hold = this.holding
    if (hold?.key !== key) return
    this.holding = null
    const back = releaseHold(hold)
    if (back != null) this.deck.stopAt(back)
  }

  private hotCue(kind: HotCueKind): Cue | undefined {
    return this.displayCues.find((c) => c.kind === 'hot' && c.slot === kind)
  }

  hotCueDown(kind: HotCueKind): void {
    const cue = this.hotCue(kind)
    if (!cue || !this.canCue) return
    const action = pressHotCue(this.state, cueRange(cue))
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

  hotCueDownOrStore(kind: HotCueKind): void {
    if (this.hotCue(kind)) this.hotCueDown(kind)
    else void this.setHotCue(kind)
  }

  hotCueUp(kind: HotCueKind): void {
    this.endHold(kind)
  }

  callCue(cue: Cue): void {
    const range = cueRange(cue)
    const loop = loopOf(range)
    if (loop) this.deck.setLoop(this.insideFile(loop[0]), loop[1])
    this.cueAt(range.start)
  }

  cueAt(seconds: Seconds): void {
    const at = this.insideFile(seconds)
    this.cuePoint = at
    if (this.playing) this.seek(at)
    else this.deck.stopAt(at)
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

  setLoopIn(): void {
    if (!this.playable) return
    const here = this.quantisedPlayhead()
    this.cuePoint = here
    if (!this.playing && !this.deck.scrubbing) this.seek(here)
  }

  setLoopOut(): void {
    if (!this.playable) return
    const start = this.cuePoint
    const end = this.quantisedPlayhead()
    if (!(end > start + SHORTEST_LOOP)) return
    this.deck.setLoop(start, end)
  }

  reloopExit(): void {
    const loop = this.loop
    if (loop) this.deck.setLooping(!loop.active)
  }

  beatLoop(beats: number): void {
    if (!this.playable) return
    const range = beatLoop(this.position, beats, this.displayGrid, this.track.bpm ?? 0, this.quantise)
    if (!range) return
    const [start, end] = range
    const loop = this.loop
    const insideActiveLoop = loop?.active && this.position >= loop.start && this.position < loop.end
    if (!insideActiveLoop) {
      this.cuePoint = start
      this.deck.setLoop(start, this.notPastEnd(end))
      return
    }
    const length = end - start
    const loopLength = loop.end - loop.start
    const pressedSameLengthAgain = Math.abs(loopLength - length) < SAME_LOOP_LENGTH_TOLERANCE
    if (pressedSameLengthAgain) this.deck.setLooping(false)
    else this.resizeLoop(length / loopLength)
  }

  resizeLoop(factor: number): void {
    const loop = this.loop
    if (!loop) return
    const resized = resizeLoop([loop.start, loop.end], factor)
    if (!resized) return
    const end = this.notPastEnd(resized[1])
    if (end > resized[0]) this.deck.setLoop(resized[0], end, loop.active)
  }

  async refreshDetail(): Promise<void> {
    await this.session.refresh()
  }

  private newCueMs(): [startMs: number, endMs: number | null] {
    const seconds = this.cuePlacement === 'cuePoint' ? this.cuePoint : this.position
    const { start, end } = cueToStore(this.quantised(seconds), this.loop)
    return [Math.round(start * 1000), end == null ? null : Math.round(end * 1000)]
  }

  setHotCue(kind: number): Promise<void> {
    return this.cues.setHotCue(kind, ...this.newCueMs())
  }

  addMemoryCue(): Promise<void> {
    return this.cues.addMemoryCue(...this.newCueMs())
  }

  private cueAtPlayhead(): Cue | null {
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
    return cue ? this.cues.deleteCue(cue) : Promise.resolve()
  }

  private cueHitTolerance(): Seconds {
    const bpm = this.trackBpmAtPlayhead
    if (!this.quantise || !(bpm > 0)) return CUE_HIT_TOLERANCE
    const halfBeat = 60 / bpm / 2
    return halfBeat
  }

  setQuantise(on: boolean): void {
    this.prefs.setQuantise(on)
  }

  quantised(seconds: Seconds): Seconds {
    if (!this.quantise) return seconds
    return nearestBeat(seconds, this.displayGrid)
  }

  private trackMs(): number {
    const span = this.waveform ? this.waveform.length / this.waveform.rate : 0
    return Math.max(this.duration, span) * 1000
  }
}
