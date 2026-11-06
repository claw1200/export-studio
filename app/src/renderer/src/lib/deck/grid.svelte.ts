import { api } from '../api'
import { beatCoord } from '../beat'
import {
  addFlag,
  compileTempoMap,
  type CompiledGrid,
  moveFlag,
  rampEndMs,
  removeFlag,
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
  timeAtCoord
} from '../tempo-map'
import type { Beatgrid, Cue, CueMove } from '../types'
import type { TempoFlagMark } from '../waveform/frame'
import type { TrackSession } from './session.svelte'
import { storedFlag, storeFlag } from './stored'

const HISTORY_LIMIT = 200

/**
 * Beatgrid editing: the grid held as tempo flags while editing, with undo, and cues that can follow it.
 * Saved or discarded as a whole.
 */
export class GridEditor {
  editing = $state(false)
  map = $state.raw<TempoMap | null>(null)
  /** The flag whose options are open, by id. */
  selected = $state<number | null>(null)
  /** A flag just added, whose options open ready to type its tempo. */
  added = $state<number | null>(null)
  /** Cues move with the grid while it is edited. */
  link = $state(storedFlag('gridLink', true))
  private history = $state.raw<TempoMap[]>([])
  private future = $state.raw<TempoMap[]>([])
  private initial: TempoMap | null = null
  /** The map a drag started from, so one drag is one undo step. */
  private gestureBase: TempoMap | null = null

  /** The per-beat grid the flags describe. */
  readonly draft: CompiledGrid | null
  /** The grid to draw: the draft while editing. */
  readonly grid: Beatgrid | null
  /** The cues to draw, moved with the draft when they are linked to it. */
  readonly cues: Cue[]
  readonly dirty: boolean
  /** The section the playhead is in. */
  readonly section: number
  /** Where flags and their ramps sit, for drawing over the waveform. Empty when not editing. */
  readonly flagMarks: TempoFlagMark[]

  constructor(
    private session: TrackSession,
    /** The playhead, in seconds. */
    private position: () => number,
    /** The track's length in ms. */
    private trackMs: () => number
  ) {
    this.draft = $derived(this.map ? compileTempoMap(this.map) : null)
    this.grid = $derived(this.draft ?? this.session.detail?.beatgrid ?? null)
    this.dirty = $derived(!!this.map && this.map !== this.initial)
    this.section = $derived(this.map ? sectionAt(this.map, this.position() * 1000) : 0)
    this.flagMarks = $derived.by(() => {
      const { map, draft } = this
      if (!map || !draft) return []
      return map.flags.map((f, j) => ({ ms: f.ms, rampEndMs: rampEndMs(draft, map, j), first: j === 0 }))
    })
    this.cues = $derived.by(() => {
      const cues = this.session.detail?.cues ?? []
      const saved = this.session.detail?.beatgrid
      const map = this.map
      const draft = this.draft
      if (!this.link || !this.dirty || !saved?.times.length || !map || !draft) return cues
      const move = (ms: number): number => Math.max(0, Math.round(timeAtCoord(map, draft, beatCoord(saved, ms))))
      return cues.map((cue) => ({ ...cue, start: move(cue.start), end: cue.end == null ? null : move(cue.end) }))
    })
  }

  get canEdit(): boolean {
    return !!this.session.detail?.beatgrid?.times.length
  }

  get canUndo(): boolean {
    return this.history.length > 0
  }

  get canRedo(): boolean {
    return this.future.length > 0
  }

  begin(): void {
    const grid = this.session.detail?.beatgrid
    if (!grid?.times.length || this.editing) return
    const map = tempoMapOf(grid, Math.max(this.trackMs(), grid.times[grid.times.length - 1]))
    if (!map) return
    this.initial = map
    this.map = map
    this.history = []
    this.future = []
    this.gestureBase = null
    this.selected = null
    this.editing = true
    this.session.editError = null
  }

  cancel(): void {
    this.editing = false
    this.map = null
    this.initial = null
    this.history = []
    this.future = []
    this.selected = null
  }

  setLink(on: boolean): void {
    this.link = on
    storeFlag('gridLink', on)
  }

  /**
   * Applies an edit. Pass `base` (the map a drag started from) to make every update of one drag a single
   * undo step; the edit is then applied to `base`, not to the current map.
   */
  private apply(edit: (map: TempoMap) => TempoMap, base?: TempoMap | null): void {
    if (!this.editing || !this.map) return
    const from = base ?? this.map
    const next = edit(from)
    if (base) {
      if (this.gestureBase !== base) {
        this.gestureBase = base
        this.push(base)
      }
    } else {
      this.gestureBase = null
      if (next === this.map) return
      this.push(this.map)
    }
    this.map = next
  }

  private push(map: TempoMap): void {
    this.history = [...this.history.slice(-HISTORY_LIMIT + 1), map]
    this.future = []
  }

  undo(): void {
    const previous = this.history[this.history.length - 1]
    if (!this.editing || !previous || !this.map) return
    this.future = [...this.future, this.map]
    this.history = this.history.slice(0, -1)
    this.gestureBase = null
    this.map = previous
    this.dropMissingSelection()
  }

  redo(): void {
    const next = this.future[this.future.length - 1]
    if (!this.editing || !next || !this.map) return
    this.history = [...this.history, this.map]
    this.future = this.future.slice(0, -1)
    this.gestureBase = null
    this.map = next
    this.dropMissingSelection()
  }

  private dropMissingSelection(): void {
    if (this.selected != null && !this.map?.flags.some((f) => f.id === this.selected)) this.selected = null
  }

  /** Shifts the section at the playhead by `ms`. */
  nudge(ms: number): void {
    const j = this.section
    this.apply((map) => shiftSection(map, j, ms))
  }

  /** Shifts section `j` of `base` by `ms`, for dragging a beat handle. */
  shiftSection(j: number, ms: number, base: TempoMap): void {
    this.apply((map) => shiftSection(map, j, ms), base)
  }

  /** Retempos section `j` of `base` so its beat `k` lands on `targetMs`, for Alt-dragging a beat handle. */
  stretch(j: number, k: number, targetMs: number, base: TempoMap): void {
    this.apply((map) => stretchSection(map, j, k, targetMs), base)
  }

  setDownbeatHere(): void {
    const ms = this.position() * 1000
    this.apply((map) => setDownbeat(map, ms))
  }

  /** Sets the tempo of the section at the playhead. Pass `base` while dragging. */
  setBpm(bpm: number, base?: TempoMap | null): void {
    if (!(Number.isFinite(bpm) && bpm > 0)) return
    const j = this.section
    const ms = this.position() * 1000
    this.apply((map) => setSectionBpm(map, j, bpm, ms), base)
  }

  halve(): void {
    this.scaleTempo(0.5)
  }

  double(): void {
    this.scaleTempo(2)
  }

  private scaleTempo(factor: number): void {
    const j = this.section
    const ms = this.position() * 1000
    this.apply((map) => setSectionBpm(map, j, map.flags[j].bpm * factor, ms, factor))
  }

  /** Adds a flag on the beat nearest `ms` and opens it. Opens the existing flag when that beat has one. */
  addFlagAt(ms: number): void {
    const map = this.map
    const draft = this.draft
    if (!this.editing || !map || !draft) return
    const added = addFlag(map, ms)
    if (added) {
      this.apply(() => added.map)
      this.selected = added.id
      this.added = added.id
      return
    }
    const flag = map.flags.find((f) => Math.abs(f.ms - ms) < 60_000 / f.bpm / 2)
    if (flag) this.selected = flag.id
  }

  addFlagHere(): void {
    this.addFlagAt(this.position() * 1000)
  }

  moveFlag(j: number, ms: number, base: TempoMap): void {
    this.apply((map) => moveFlag(map, j, ms), base)
  }

  setFlagBpm(j: number, bpm: number): void {
    if (Number.isFinite(bpm) && bpm > 0) this.apply((map) => setFlagBpm(map, j, bpm))
  }

  /** Ramp length in beats; 0 makes the flag a jump. Pass `base` while dragging the ramp's end. */
  setRamp(j: number, beats: number, base?: TempoMap | null): void {
    this.apply((map) => setRamp(map, j, beats), base)
  }

  setBarStart(j: number, on: boolean): void {
    this.apply((map) => setBarStart(map, j, on))
  }

  removeFlag(j: number): void {
    this.apply((map) => removeFlag(map, j))
    this.dropMissingSelection()
  }

  select(id: number | null): void {
    this.selected = id
  }

  /** Time of the flag before or after the playhead, in seconds, or null. */
  flagNear(direction: -1 | 1): number | null {
    const ms = this.position() * 1000
    const times = this.map?.flags.map((f) => f.ms) ?? []
    const found = direction < 0 ? times.findLast((t) => t < ms - 50) : times.find((t) => t > ms + 50)
    return found == null ? null : found / 1000
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
    const grid = {
      times: draft.times.map((t) => Math.round(t)),
      beats: draft.beats,
      bpms: draft.bpms.map((b) => Math.round(b * 100) / 100)
    }
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
