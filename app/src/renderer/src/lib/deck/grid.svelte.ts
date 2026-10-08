import { api } from '../api'
import { bpmAt, draftOf, type GridDraft, remapTime, retempoGrid, setDownbeat, shiftGrid } from '../beat'
import type { Beatgrid, Cue, CueMove } from '../types'
import type { TrackSession } from './session.svelte'
import { storedFlag, storeFlag } from './stored'

/** Beatgrid editing: a draft grid that cues can follow, saved or discarded as a whole. */
export class GridEditor {
  editing = $state(false)
  draft = $state.raw<GridDraft | null>(null)
  dirty = $state(false)
  /** Cues move with the grid while it is edited. */
  link = $state(storedFlag('gridLink', true))
  /** The grid to draw: the draft while editing. */
  readonly grid: Beatgrid | null
  /** The cues to draw, moved with the draft when they are linked to it. */
  readonly cues: Cue[]

  constructor(
    private session: TrackSession,
    /** The playhead, in seconds. */
    private position: () => number,
    /** The track's length in ms. */
    private trackMs: () => number
  ) {
    this.grid = $derived(this.draft ?? this.session.detail?.beatgrid ?? null)
    this.cues = $derived.by(() => {
      const cues = this.session.detail?.cues ?? []
      const saved = this.session.detail?.beatgrid
      const draft = this.draft
      if (!this.link || !this.dirty || !saved || !draft) return cues
      const move = (ms: number): number => Math.max(0, Math.round(remapTime(ms, saved, draft)))
      return cues.map((cue) => ({ ...cue, start: move(cue.start), end: cue.end == null ? null : move(cue.end) }))
    })
  }

  get canEdit(): boolean {
    return !!this.session.detail?.beatgrid?.times.length
  }

  begin(): void {
    const grid = this.session.detail?.beatgrid
    if (!grid?.times.length || this.editing) return
    this.draft = draftOf(grid)
    this.dirty = false
    this.editing = true
    this.session.editError = null
  }

  cancel(): void {
    this.editing = false
    this.draft = null
    this.dirty = false
  }

  setLink(on: boolean): void {
    this.link = on
    storeFlag('gridLink', on)
  }

  private update(draft: GridDraft): void {
    this.draft = draft
    this.dirty = true
  }

  /** Shifts `base` (the current draft by default) by `ms`, so drags can work from their starting grid. */
  nudge(ms: number, base: GridDraft | null = this.draft): void {
    if (!this.editing || !base) return
    this.update(shiftGrid(base, ms, this.trackMs()))
  }

  setDownbeatHere(): void {
    const draft = this.draft
    if (!this.editing || !draft) return
    this.update(setDownbeat(draft, this.position() * 1000, this.trackMs()))
  }

  private retempo(bpm: (current: number) => number, density = 1, draft = this.draft): void {
    if (!this.editing || !draft) return
    const current = bpmAt(this.position(), draft)
    if (!(current > 0)) return
    this.update(retempoGrid(draft, bpm(current), this.position() * 1000, this.trackMs(), density))
  }

  /** Sets the tempo of `base` (the current draft by default), so drags can work from their starting grid. */
  setBpm(bpm: number, base: GridDraft | null = this.draft): void {
    if (Number.isFinite(bpm) && bpm > 0) this.retempo(() => bpm, 1, base)
  }

  halve(): void {
    this.retempo((current) => current / 2, 0.5)
  }

  double(): void {
    this.retempo((current) => current * 2, 2)
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
    const grid = { times: draft.times.map((t) => Math.round(t)), beats: draft.beats, bpms: draft.bpms }
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
