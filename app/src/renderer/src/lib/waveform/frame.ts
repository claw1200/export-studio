import type { Beatgrid, Cue } from '../types'
import type { Waveform } from './data'

/** What the scrolling detail waveform shows around the playhead. */
export interface DetailView {
  position: number
  zoom: number
  cues: Cue[]
  beatgrid: Beatgrid | null
  cuePoint: number | null
  /** The deck's loop in seconds; drawn brighter while playback is held in it. */
  loop: { start: number; end: number; active: boolean } | null
  gridEditing: boolean
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
  toX: (seconds: number) => number
}
