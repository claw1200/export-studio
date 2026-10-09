/** What the detail view draws over the waveform: loops, the beatgrid, cues and the playhead. */
import { barBeatAt, countdownTo, firstIndexAtOrAfter } from '../beat'
import { cueColor, hotCueLetter } from '../format'
import type { Beatgrid, Cue } from '../types'
import { CUE_POINT_COLOR, LOOP_ACTIVE_COLOR } from './colors'
import type { Waveform } from './data'
import type { Frame } from './frame'

/** Beat handles, in CSS pixels. A vertical pill centered on each grid line, with a slightly larger hit box. */
const HANDLE_W = 6
const HANDLE_H = 30
const HANDLE_HIT_X = 10
const HANDLE_HIT_Y = 18

export function paintLoops(frame: Frame): void {
  const { ctx, view, pw, ph, toX } = frame
  for (const cue of view.cues) {
    if (cue.end == null) continue
    const x0 = toX(cue.start / 1000)
    const x1 = toX(cue.end / 1000)
    if (x1 < 0 || x0 > pw) continue
    ctx.fillStyle = cueColor(cue) + '33'
    ctx.fillRect(x0, 0, x1 - x0, ph)
  }
  const loop = view.loop
  if (!loop) return
  const x0 = toX(loop.start)
  const x1 = toX(loop.end)
  if (x1 < 0 || x0 > pw) return
  ctx.fillStyle = LOOP_ACTIVE_COLOR + (loop.active ? '40' : '18')
  ctx.fillRect(x0, 0, x1 - x0, ph)
  ctx.fillStyle = LOOP_ACTIVE_COLOR
  const edge = Math.max(1, Math.round(frame.dpr))
  ctx.fillRect(Math.round(x0), 0, edge, ph)
  ctx.fillRect(Math.round(x1) - edge, 0, edge, ph)
}

export function paintBeatGrid(frame: Frame): void {
  const { ctx, wf, view, ph, dpr, from, to, toX } = frame
  const grid = view.beatgrid
  if (!grid) return
  const startMs = (from / wf.rate) * 1000
  const endMs = (to / wf.rate) * 1000
  const first = firstIndexAtOrAfter(grid.times, startMs)
  const line = Math.max(1, dpr)
  if (view.gridEditing) {
    for (let i = first; i < grid.times.length && grid.times[i] <= endMs; i++) {
      const x = Math.round(toX(grid.times[i] / 1000))
      ctx.fillStyle = grid.beats[i] === 1 ? 'rgba(255,72,88,0.95)' : 'rgba(255,255,255,0.55)'
      ctx.fillRect(x, 0, line, ph)
    }
    return
  }
  const cueXs = view.cues.map((cue) => Math.round(toX(cue.start / 1000)))
  const coveredByCue = (x: number): boolean => cueXs.some((cueX) => Math.abs(cueX - x) <= 2 * dpr)
  for (let i = first; i < grid.times.length && grid.times[i] <= endMs; i++) {
    const x = Math.round(toX(grid.times[i] / 1000))
    if (coveredByCue(x)) continue
    const downbeat = grid.beats[i] === 1
    const tick = (downbeat ? 7 : 4) * dpr
    ctx.fillStyle = downbeat ? 'rgba(255,72,88,0.9)' : 'rgba(255,255,255,0.28)'
    ctx.fillRect(x, 0, line, tick)
    ctx.fillRect(x, ph - tick, line, tick)
    if (downbeat) {
      ctx.fillStyle = 'rgba(255,255,255,0.10)'
      ctx.fillRect(x, 0, line, ph)
    }
  }
}

export function gridHandleIndex(
  wf: Waveform,
  position: number,
  zoom: number,
  grid: Beatgrid | null,
  cssWidth: number,
  cssHeight: number,
  x: number,
  y: number
): number {
  if (!grid?.times.length || !(cssWidth > 0) || !(cssHeight > 0) || !(zoom > 0) || !(wf.rate > 0)) return -1
  if (Math.abs(y - cssHeight / 2) > HANDLE_HIT_Y) return -1
  const pxPerSecond = wf.rate * zoom
  const half = cssWidth / 2
  const startMs = (position - half / pxPerSecond) * 1000
  const endMs = (position + half / pxPerSecond) * 1000
  const first = firstIndexAtOrAfter(grid.times, startMs)
  let best = -1
  let bestDx = Infinity
  for (let i = first; i < grid.times.length && grid.times[i] <= endMs; i++) {
    const dx = Math.abs(x - (half + (grid.times[i] / 1000 - position) * pxPerSecond))
    if (dx <= HANDLE_HIT_X && dx < bestDx) {
      bestDx = dx
      best = i
    }
  }
  return best
}

/** A vertical pill on each beat, centered on the grid line. Dragging one shifts the grid. */
export function paintGridHandles(frame: Frame): void {
  const { ctx, view, dpr, ph, toX, wf, from, to } = frame
  const grid = view.beatgrid
  if (!view.gridEditing || !grid) return
  const line = Math.max(1, dpr)
  const w = HANDLE_W * dpr
  const h = HANDLE_H * dpr
  const y = ph / 2
  const startMs = (from / wf.rate) * 1000
  const endMs = (to / wf.rate) * 1000
  const first = firstIndexAtOrAfter(grid.times, startMs)
  for (let i = first; i < grid.times.length && grid.times[i] <= endMs; i++) {
    const x = Math.round(toX(grid.times[i] / 1000)) + line / 2
    const downbeat = grid.beats[i] === 1
    const hot = i === view.hotGridIndex
    ctx.beginPath()
    ctx.roundRect(x - w / 2, y - h / 2, w, h, w / 2)
    ctx.fillStyle = downbeat ? (hot ? '#ff6174' : '#ff4858') : hot ? '#ffffff' : 'rgba(255,255,255,0.92)'
    ctx.fill()
    ctx.lineWidth = Math.max(1, dpr)
    ctx.strokeStyle = 'rgba(0,0,0,0.5)'
    ctx.stroke()
  }
}

function memoryCueBeforeHotCue(a: Cue, b: Cue): number {
  return Number(a.kind === 'hot') - Number(b.kind === 'hot')
}

export function paintCues(frame: Frame): void {
  const { ctx, view, pw, ph, dpr, toX } = frame
  const diamond = 6 * dpr
  const stem = Math.max(1, dpr)
  const hotStem = Math.max(2, Math.round(1.5 * dpr))
  const tipHeight = Math.round(diamond * 1.2)
  const badge = Math.round(14 * dpr)
  ctx.font = `700 ${Math.round(9.5 * dpr)}px "Inter Variable", system-ui, sans-serif`
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  for (const cue of [...view.cues].sort(memoryCueBeforeHotCue)) {
    const x = Math.round(toX(cue.start / 1000))
    if (x < -40 * dpr || x > pw + 40 * dpr) continue
    ctx.fillStyle = cueColor(cue)
    if (cue.kind === 'hot') {
      ctx.fillRect(x, 0, hotStem, ph)
      ctx.fillRect(x, 0, badge, badge)
      ctx.fillStyle = 'rgba(0,0,0,0.85)'
      ctx.fillText(hotCueLetter(cue.slot), x + badge / 2, badge / 2 + 0.5 * dpr)
      continue
    }
    const tip = x + stem / 2
    ctx.fillRect(x, 0, stem, ph)
    ctx.beginPath()
    ctx.moveTo(tip - diamond, 0)
    ctx.lineTo(tip + diamond, 0)
    ctx.lineTo(tip, tipHeight)
    ctx.closePath()
    ctx.fill()
  }
}

export function paintCuePoint(frame: Frame): void {
  const { ctx, view, pw, ph, dpr, toX } = frame
  if (view.cuePoint == null) return
  const x = Math.round(toX(view.cuePoint))
  const size = 7 * dpr
  if (x < -size || x > pw + size) return
  const tipHeight = Math.round(size * 1.2)
  ctx.fillStyle = CUE_POINT_COLOR
  ctx.beginPath()
  ctx.moveTo(x - size, ph)
  ctx.lineTo(x + size, ph)
  ctx.lineTo(x, ph - tipHeight)
  ctx.closePath()
  ctx.fill()
}

export function paintPlayhead(frame: Frame): void {
  const { ctx, pw, ph, dpr } = frame
  const x = Math.round(pw / 2)
  ctx.fillStyle = 'rgba(0,0,0,0.5)'
  ctx.fillRect(x - dpr, 0, 3 * dpr, ph)
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(x, 0, Math.max(1, dpr), ph)
}

function nextCue(cues: Cue[], seconds: number): Cue | null {
  const ms = seconds * 1000 + 1
  let next: Cue | null = null
  for (const cue of cues) {
    if (cue.start > ms && (!next || cue.start < next.start)) next = cue
  }
  return next
}

function paintPill(frame: Frame, y: number, text: string, cue: Cue | null): void {
  const { ctx, pw, dpr } = frame
  const height = Math.round(15 * dpr)
  const pad = Math.round(5 * dpr)
  const chip = cue ? Math.round(9 * dpr) : 0
  const gap = cue ? Math.round(4 * dpr) : 0
  const textWidth = Math.ceil(ctx.measureText(text).width)
  const width = pad * 2 + chip + gap + textWidth
  const left = Math.round(pw / 2 - width / 2)
  const mid = y + height / 2

  ctx.fillStyle = 'rgba(10,11,14,0.82)'
  ctx.beginPath()
  ctx.roundRect(left, y, width, height, 3 * dpr)
  ctx.fill()
  ctx.strokeStyle = 'rgba(255,255,255,0.18)'
  ctx.lineWidth = Math.max(1, dpr)
  ctx.stroke()

  if (cue) {
    const cx = left + pad
    ctx.fillStyle = cueColor(cue)
    if (cue.kind === 'hot') {
      ctx.fillRect(cx, mid - chip / 2, chip, chip)
      ctx.save()
      ctx.font = `800 ${Math.round(7.5 * dpr)}px "Inter Variable", system-ui, sans-serif`
      ctx.textAlign = 'center'
      ctx.fillStyle = 'rgba(0,0,0,0.85)'
      ctx.fillText(hotCueLetter(cue.slot), cx + chip / 2, mid + 0.5 * dpr)
      ctx.restore()
    } else {
      ctx.beginPath()
      ctx.moveTo(cx, mid - chip / 2)
      ctx.lineTo(cx + chip, mid - chip / 2)
      ctx.lineTo(cx + chip / 2, mid + chip / 2)
      ctx.closePath()
      ctx.fill()
    }
  }

  ctx.fillStyle = '#ffffff'
  ctx.textAlign = 'left'
  ctx.fillText(text, left + pad + chip + gap, mid + 0.5 * dpr)
}

/** Bar and beat at the playhead, and the beats left to the next cue. */
export function paintBeatCounters(frame: Frame): void {
  const { ctx, view, ph, dpr } = frame
  const position = barBeatAt(view.position, view.beatgrid)
  const cue = nextCue(view.cues, view.position)
  const countdown = cue ? countdownTo(view.position, cue.start / 1000, view.beatgrid) : null
  if (!position && !countdown) return
  ctx.font = `600 ${Math.round(10 * dpr)}px ui-monospace, "SF Mono", Menlo, Consolas, monospace`
  ctx.textBaseline = 'middle'
  const inset = Math.round(10 * dpr)
  if (countdown) paintPill(frame, inset, `−${countdown}`, cue)
  if (position) paintPill(frame, ph - inset - Math.round(15 * dpr), position, null)
}
