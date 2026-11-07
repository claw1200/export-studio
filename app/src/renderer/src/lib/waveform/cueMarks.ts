/**
 * Cue marks on a whole-track waveform: a square with its letter for each hot cue, a triangle for
 * each memory cue, and a tint over each cue's loop. `WaveformMarks` draws them as DOM buttons;
 * `drawCueMarks` paints the same marks into a canvas, for browser rows that are not pointed at.
 */
import { cueColor, hotCueLetter } from '../format'
import type { Cue } from '../types'

/** What both the DOM and canvas marks are built from, in CSS pixels. */
export const CUE_MARK = {
  hotSize: 12,
  hotFontSize: 8,
  hotTextColor: 'rgba(0,0,0,0.85)',
  memoryWidth: 9,
  memoryHeight: 7,
  loopOpacity: 0.25
} as const

const HOT_FONT = `700 ${CUE_MARK.hotFontSize}px 'Inter Variable', Inter, ui-sans-serif, system-ui, sans-serif`

function paintLoopTints(
  ctx: CanvasRenderingContext2D,
  cues: readonly Cue[],
  xOf: (ms: number) => number,
  height: number
): void {
  ctx.globalAlpha = CUE_MARK.loopOpacity
  for (const cue of cues) {
    if (cue.end == null) continue
    const left = xOf(cue.start)
    ctx.fillStyle = cueColor(cue)
    ctx.fillRect(left, 0, xOf(cue.end) - left, height)
  }
  ctx.globalAlpha = 1
}

function paintMemoryTriangle(ctx: CanvasRenderingContext2D, cue: Cue, x: number): void {
  const half = CUE_MARK.memoryWidth / 2
  ctx.fillStyle = cueColor(cue)
  ctx.beginPath()
  ctx.moveTo(x - half, 0)
  ctx.lineTo(x + half, 0)
  ctx.lineTo(x, CUE_MARK.memoryHeight)
  ctx.closePath()
  ctx.fill()
}

function paintHotSquare(ctx: CanvasRenderingContext2D, cue: Cue, x: number): void {
  const size = CUE_MARK.hotSize
  ctx.fillStyle = cueColor(cue)
  ctx.fillRect(x, 0, size, size)
  ctx.fillStyle = CUE_MARK.hotTextColor
  ctx.fillText(hotCueLetter(cue.slot), x + size / 2, size / 2 + 0.5)
}

/**
 * `cues` across a `width` × `height` CSS-pixel canvas showing a track `span` seconds long.
 * Hot cues are painted last, so they sit on top as their buttons do.
 */
export function drawCueMarks(
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
  paintLoopTints(ctx, cues, xOf, height)
  for (const cue of cues) if (cue.kind !== 'hot') paintMemoryTriangle(ctx, cue, xOf(cue.start))
  ctx.font = HOT_FONT
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  for (const cue of cues) if (cue.kind === 'hot') paintHotSquare(ctx, cue, xOf(cue.start))
  ctx.setTransform(1, 0, 0, 1, 0, 0)
}
