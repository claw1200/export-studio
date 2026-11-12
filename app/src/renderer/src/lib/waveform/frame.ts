import type { Beatgrid, Cue } from '../types'
import type { Waveform } from './data'

export interface TempoFlagMark {
  startMs: number
  rampEndMs: number | null
  first: boolean
}

export type WidthFraction = number

/** What the scrolling detail waveform shows around the playhead. */
export interface DetailView {
  position: number
  playheadAt: WidthFraction
  zoom: number
  cues: Cue[]
  beatgrid: Beatgrid | null
  cuePoint: number | null
  /** The deck's loop in seconds; drawn brighter while playback is held in it. */
  loop: { start: number; end: number; active: boolean } | null
  gridEditing: boolean
  /** Beat under the pointer while editing, or -1. Dragging its handle shifts the grid. */
  hotGridIndex: number
  tempoFlags?: TempoFlagMark[]
}

/** One draw of the detail view: the canvas in device pixels and the entries it spans. */
export interface Frame {
  ctx: CanvasRenderingContext2D
  wf: Waveform
  view: DetailView
  pw: number
  ph: number
  dpr: number
  from: number
  to: number
  playheadX: number
  toX: (seconds: number) => number
}
