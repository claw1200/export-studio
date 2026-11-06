import { api } from '../api'
import { fractionalBeatIndex, nearestBeatIndex } from '../beat'
import {
  addFlag,
  compileTempoMap,
  type CompiledGrid,
  moveFlag,
  rampEndMs,
  removeFlag,
  roundBpm,
  sectionAt,
  setBarStart,
  setDownbeat,
  setFlagBpm,
  setRamp,
  setSectionBpm,
  shiftSection,
  stretchSection,
  type TempoMap,
  tempoMapOf,
  timeOfSavedBeat
} from '../tempo-map'
import type { Beatgrid, Cue, CueMove } from '../types'
import type { TempoFlagMark } from '../waveform/frame'
import type { TrackSession } from './session.svelte'
import { storedFlag, storeFlag } from './stored'

const HISTORY_LIMIT = 200
const FLAG_JUMP_MARGIN_MS = 50

/** Beatgrid editing: the grid held as tempo flags, saved or discarded as a whole. */
export class GridEditor {
  editing = $state(false)
  map = $state.raw<TempoMap | null>(null)
  selectedFlagId = $state<number | null>(null)
  justAddedFlagId = $state<number | null>(null)
  /** Cues move with the grid while it is edited. */
  link = $state(storedFlag('gridLink', true))
  private undoStack = $state.raw<TempoMap[]>([])
  private redoStack = $state.raw<TempoMap[]>([])
  private loadedMap: TempoMap | null = null
  private dragStartMap: TempoMap | null = null

  readonly draft: CompiledGrid | null
  /** The grid to draw: the draft while editing. */
  readonly grid: Beatgrid | null
  /** The cues to draw, moved with the draft when they are linked to it. */
  readonly cues: Cue[]
  readonly dirty: boolean
  readonly sectionAtPlayhead: number
  readonly flagMarks: TempoFlagMark[]

  constructor(
    private session: TrackSession,
    /** The playhead, in seconds. */
    private position: () => number,
    /** The track's length in ms. */
    private trackMs: () => number,
    private seek: (seconds: number) => void
  ) {
    this.draft = $derived(this.map ? compileTempoMap(this.map) : null)
    this.grid = $derived(this.draft ?? this.session.detail?.beatgrid ?? null)
    this.dirty = $derived(!!this.map && this.map !== this.loadedMap)
    this.sectionAtPlayhead = $derived(this.map ? sectionAt(this.map, this.playheadMs) : 0)
    this.flagMarks = $derived.by(() => {
      const { map, draft } = this
      if (!map || !draft) return []
      return map.flags.map((flag, j) => ({
        startMs: flag.startMs,
        rampEndMs: rampEndMs(draft, map, j),
        first: j === 0
      }))
    })
    this.cues = $derived.by(() => {
      const cues = this.session.detail?.cues ?? []
      const saved = this.session.detail?.beatgrid
      const { map, draft } = this
      if (!this.link || !this.dirty || !saved?.times.length || !map || !draft) return cues
      const move = (ms: number): number =>
        Math.max(0, Math.round(timeOfSavedBeat(map, draft, fractionalBeatIndex(saved, ms))))
      return cues.map((cue) => ({ ...cue, start: move(cue.start), end: cue.end == null ? null : move(cue.end) }))
    })
  }

  private get playheadMs(): number {
    return this.position() * 1000
  }

  get canEdit(): boolean {
    return !!this.session.detail?.beatgrid?.times.length
  }

  get canUndo(): boolean {
    return this.undoStack.length > 0
  }

  get canRedo(): boolean {
    return this.redoStack.length > 0
  }

  begin(): void {
    const grid = this.session.detail?.beatgrid
    if (!grid?.times.length || this.editing) return
    const map = tempoMapOf(grid, Math.max(this.trackMs(), grid.times[grid.times.length - 1]))
    if (!map) return
    this.reset(map)
    this.editing = true
    this.session.editError = null
  }

  cancel(): void {
    this.reset(null)
    this.editing = false
  }

  private reset(map: TempoMap | null): void {
    this.loadedMap = map
    this.map = map
    this.undoStack = []
    this.redoStack = []
    this.dragStartMap = null
    this.selectedFlagId = null
  }

  setLink(on: boolean): void {
    this.link = on
    storeFlag('gridLink', on)
  }

  /** Every update of one drag passes the same `dragStart`, so the whole drag is one undo step. */
  private apply(edit: (map: TempoMap) => TempoMap, dragStart?: TempoMap | null): void {
    if (!this.editing || !this.map) return
    const next = edit(dragStart ?? this.map)
    if (dragStart) {
      if (this.dragStartMap !== dragStart) this.pushUndo(dragStart)
      this.dragStartMap = dragStart
    } else {
      this.dragStartMap = null
      if (next === this.map) return
      this.pushUndo(this.map)
    }
    this.map = next
  }

  private pushUndo(map: TempoMap): void {
    this.undoStack = [...this.undoStack.slice(1 - HISTORY_LIMIT), map]
    this.redoStack = []
  }

  undo(): void {
    this.restore('undoStack', 'redoStack')
  }

  redo(): void {
    this.restore('redoStack', 'undoStack')
  }

  private restore(from: 'undoStack' | 'redoStack', to: 'undoStack' | 'redoStack'): void {
    const target = this[from].at(-1)
    if (!this.editing || !target || !this.map) return
    this[to] = [...this[to], this.map]
    this[from] = this[from].slice(0, -1)
    this.dragStartMap = null
    this.map = target
    if (!target.flags.some((flag) => flag.id === this.selectedFlagId)) this.selectedFlagId = null
  }

  nudge(ms: number): void {
    const j = this.sectionAtPlayhead
    this.apply((map) => shiftSection(map, j, ms))
  }

  shiftSection(j: number, ms: number, dragStart: TempoMap): void {
    this.apply((map) => shiftSection(map, j, ms), dragStart)
  }

  stretch(j: number, step: number, targetMs: number, dragStart: TempoMap): void {
    this.apply((map) => stretchSection(map, j, step, targetMs), dragStart)
  }

  setDownbeatHere(): void {
    const ms = this.playheadMs
    this.apply((map) => setDownbeat(map, ms))
  }

  setBpm(bpm: number, dragStart?: TempoMap | null): void {
    if (Number.isFinite(bpm) && bpm > 0) this.scaleTempo(() => bpm, 1, dragStart)
  }

  halve(): void {
    this.scaleTempo((bpm) => bpm / 2, 0.5)
  }

  double(): void {
    this.scaleTempo((bpm) => bpm * 2, 2)
  }

  private scaleTempo(tempo: (bpm: number) => number, density: number, dragStart?: TempoMap | null): void {
    const j = this.sectionAtPlayhead
    const ms = this.playheadMs
    this.apply((map) => setSectionBpm(map, j, tempo(map.flags[j].bpm), ms, density), dragStart)
  }

  addFlagAt(ms: number): void {
    const { map, draft } = this
    if (!this.editing || !map || !draft) return
    const added = addFlag(map, ms)
    if (added) {
      this.apply(() => added.map)
      this.selectedFlagId = this.justAddedFlagId = added.id
      return
    }
    const at = nearestBeatIndex(draft.times, ms)
    if (draft.stepInSection[at] === 0) this.selectedFlagId = map.flags[draft.sectionOfBeat[at]].id
  }

  addFlagAtPlayhead(): void {
    this.addFlagAt(this.playheadMs)
  }

  moveFlag(j: number, ms: number, dragStart: TempoMap): void {
    this.apply((map) => moveFlag(map, j, ms), dragStart)
  }

  setFlagBpm(j: number, bpm: number): void {
    if (Number.isFinite(bpm) && bpm > 0) this.apply((map) => setFlagBpm(map, j, bpm))
  }

  setRamp(j: number, beats: number, dragStart?: TempoMap | null): void {
    this.apply((map) => setRamp(map, j, beats), dragStart)
  }

  setBarStart(j: number, on: boolean): void {
    this.apply((map) => setBarStart(map, j, on))
  }

  removeFlag(j: number): void {
    this.apply((map) => removeFlag(map, j))
    this.selectedFlagId = null
  }

  select(id: number | null): void {
    this.selectedFlagId = id
  }

  jumpToFlag(direction: -1 | 1): void {
    const ms = this.playheadMs
    const starts = this.map?.flags.map((flag) => flag.startMs) ?? []
    const target =
      direction < 0
        ? starts.findLast((start) => start < ms - FLAG_JUMP_MARGIN_MS)
        : starts.find((start) => start > ms + FLAG_JUMP_MARGIN_MS)
    if (target != null) this.seek(target / 1000)
  }

  private movedCues(): CueMove[] {
    if (!this.link) return []
    const before = new Map((this.session.detail?.cues ?? []).map((c) => [c.id, c]))
    return this.cues
      .filter((c) => {
        const was = before.get(c.id)
        return was && (was.start !== c.start || was.end !== c.end)
      })
      .map((c) => ({ id: c.id, start: c.start, end: c.end }))
  }

  async save(): Promise<void> {
    const draft = this.draft
    if (!draft) return
    if (!this.dirty) {
      this.cancel()
      return
    }
    const grid = { times: draft.times.map(Math.round), beats: draft.beats, bpms: draft.bpms.map(roundBpm) }
    const moves = this.movedCues()
    const saved = await this.session.edit(
      (id) => api.setBeatgrid(id, grid, moves),
      (detail, result) => ({ ...detail, beatgrid: result.beatgrid, cues: result.cues })
    )
    if (!saved) return
    const track = this.session.track
    if (saved.bpm != null && track) this.session.track = { ...track, bpm: saved.bpm }
    this.cancel()
  }
}
