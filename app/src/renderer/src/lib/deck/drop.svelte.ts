import type { Library } from '../library.svelte'
import type { Track } from '../types'

/**
 * A deck as somewhere to drop a track dragged from the library. Several rows dragged together
 * load the one the drag was started on. `over` is true while a droppable drag is above it, for
 * the deck to highlight itself.
 */
export class TrackDrop {
  over = $state(false)
  /** dragenter and dragleave fire for every child crossed, so count them rather than toggle. */
  private depth = 0

  constructor(
    private library: Library,
    private onload: (track: Track) => void
  ) {}

  /** The track a drop here would load, or null when what is being dragged is not one. */
  private get track(): Track | null {
    const ids = this.library.dragging
    if (!ids?.length) return null
    const id = this.library.dragAnchor && ids.includes(this.library.dragAnchor) ? this.library.dragAnchor : ids[0]
    return this.library.byId.get(id) ?? null
  }

  ondragenter = (e: DragEvent): void => {
    if (!this.track) return
    e.preventDefault()
    this.depth++
    this.over = true
  }

  ondragover = (e: DragEvent): void => {
    if (!this.track) return
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
  }

  ondragleave = (): void => {
    if (!this.over) return
    this.depth = Math.max(0, this.depth - 1)
    if (!this.depth) this.over = false
  }

  ondrop = (e: DragEvent): void => {
    const track = this.track
    this.depth = 0
    this.over = false
    if (!track) return
    e.preventDefault()
    this.onload(track)
  }
}
