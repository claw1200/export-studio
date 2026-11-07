/**
 * Cue marks painted into a browser waveform thumbnail, matching what `WaveformMarks` draws as DOM.
 * Rows only mount the interactive buttons while pointed at; the rest of the time the canvas shows
 * the marks, so a scrolled-in row has them at once without building a button per cue.
 */
import { cueColor, hotCueLetter } from '../format'
import type { Cue } from '../types'

/** Hot cue square, in CSS pixels (`size-3`). */
const HOT_SIZE = 12
/** Memory cue triangle, in CSS pixels (`w-[9px] h-[7px]`). */
const MEMORY_W = 9
const MEMORY_H = 7
const HOT_FONT = "700 8px 'Inter Variable', Inter, ui-sans-serif, system-ui, sans-serif"

/**
 * `cues` across a `width` × `height` CSS-pixel thumbnail of a track `span` seconds long. The
 * context's transform is reset afterwards.
 */
export function drawThumbMarks(
  ctx: CanvasRenderingContext2D,
  cues: readonly Cue[],
  span: number,
  width: number,
  height: number,
  dpr: number
): void {
  if (!cues.length || !(span > 0)) return
  const xOf = (ms: number): number => Math.max(0, Math.min(1, ms / 1000 / span)) * width
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

  ctx.globalAlpha = 0.25
  for (const cue of cues) {
    if (cue.end == null) continue
    const x0 = xOf(cue.start)
    ctx.fillStyle = cueColor(cue)
    ctx.fillRect(x0, 0, xOf(cue.end) - x0, height)
  }
  ctx.globalAlpha = 1

  for (const cue of cues) {
    if (cue.kind === 'hot') continue
    const x = xOf(cue.start)
    ctx.fillStyle = cueColor(cue)
    ctx.beginPath()
    ctx.moveTo(x - MEMORY_W / 2, 0)
    ctx.lineTo(x + MEMORY_W / 2, 0)
    ctx.lineTo(x, MEMORY_H)
    ctx.closePath()
    ctx.fill()
  }

  ctx.font = HOT_FONT
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  for (const cue of cues) {
    if (cue.kind !== 'hot') continue
    const x = xOf(cue.start)
    ctx.fillStyle = cueColor(cue)
    ctx.fillRect(x, 0, HOT_SIZE, HOT_SIZE)
    ctx.fillStyle = 'rgba(0,0,0,0.85)'
    ctx.fillText(hotCueLetter(cue.slot), x + HOT_SIZE / 2, HOT_SIZE / 2 + 0.5)
  }

  ctx.setTransform(1, 0, 0, 1, 0, 0)
}
