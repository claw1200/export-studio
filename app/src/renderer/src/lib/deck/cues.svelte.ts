import { api } from '../api'
import type { Cue } from '../types'
import type { GridEditor } from './grid.svelte'
import type { TrackSession } from './session.svelte'

/** Library writes to the track's hot cues and memory cues. Locked while the grid is being edited. */
export class CueEditor {
  constructor(
    private session: TrackSession,
    private grid: GridEditor
  ) {}

  setHotCue(kind: number, ms: number): Promise<void> {
    return this.write((id) => api.setHotCue(id, kind, ms))
  }

  addMemoryCue(ms: number): Promise<void> {
    return this.write((id) => api.addMemoryCue(id, ms))
  }

  deleteCue(cue: Cue): Promise<void> {
    return this.write((id) => api.deleteCue(id, cue.id))
  }

  setCueColor(cue: Cue, colorIndex: number | null): Promise<void> {
    return this.write((id) => api.setCueColor(id, cue.id, colorIndex))
  }

  private async write(write: (id: string) => Promise<Cue[]>): Promise<void> {
    if (this.grid.editing) return
    await this.session.edit(write, (detail, cues) => ({ ...detail, cues }))
  }
}
