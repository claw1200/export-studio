/**
 * Waveform data read into one value per pixel column, and those columns painted as a shape.
 * The column buffers are shared between draws and only grow.
 */

export const columns = {
  amp: new Float32Array(0),
  /** A packed colour, or an id to group by; negative for an empty column. */
  color: new Int32Array(0)
}

export function fitColumns(n: number): void {
  if (columns.amp.length >= n) return
  const size = Math.max(n, columns.amp.length * 2)
  columns.amp = new Float32Array(size)
  columns.color = new Int32Array(size)
}

export function packColor(colors: Uint8Array, index: number): number {
  const o = index * 3
  return (colors[o] << 16) | (colors[o + 1] << 8) | colors[o + 2]
}

/** Entries per pixel for `from`–`to` across `pw` pixels, and the pixel `from` falls in. */
export function pixelGrid(from: number, to: number, pw: number): { per: number; first: number } | null {
  const span = to - from
  if (!(span > 0) || pw < 1) return null
  const per = span / pw
  if (!(per > 0)) return null
  return { per, first: Math.floor(from / per) }
}

/** Entries left after reading every `stride`th group. */
export function logicalCount(src: number, stride: number): number {
  return stride <= 1 ? src : Math.max(1, Math.floor(src / stride))
}

/** Loudest byte of one band in the stride window that starts at `index`. */
export function stridePeak(data: Uint8Array, src: number, index: number, stride: number, band: number): number {
  const a = Math.min(src, Math.floor(index * stride))
  const b = Math.min(src, Math.max(a + 1, Math.ceil((index + 1) * stride)))
  let peak = 0
  for (let j = a; j < b; j++) {
    const value = data[j * 3 + band]
    if (value > peak) peak = value
  }
  return peak
}

type ColumnSpan =
  { kind: 'hold'; index: number; next: number; fraction: number } | { kind: 'peak'; from: number; to: number }

function columnSpan(count: number, start: number, end: number): ColumnSpan | null {
  if (!(end > start) || end <= 0 || start >= count) return null
  if (end - start <= 1) {
    const at = Math.min(count - 1, Math.max(0, (start + end) / 2))
    const index = at | 0
    return { kind: 'hold', index, next: Math.min(count - 1, index + 1), fraction: at - index }
  }
  return { kind: 'peak', from: Math.max(0, Math.floor(start)), to: Math.min(count, Math.ceil(end)) }
}

/** Slopes down toward the next block. A rise stays flat until the block boundary. */
function fall(current: number, next: number, fraction: number): number {
  if (next >= current) return current
  return current + (next - current) * fraction
}

/** The peak of entries `start`–`end`, or NaN outside the data. */
export function readAmp(
  get: (i: number) => number,
  count: number,
  start: number,
  end: number,
  lerpDown: boolean
): number {
  const span = columnSpan(count, start, end)
  if (!span) return NaN
  if (span.kind === 'hold') {
    return lerpDown ? fall(get(span.index), get(span.next), span.fraction) : get(span.index)
  }
  let peak = 0
  for (let i = span.from; i < span.to; i++) {
    const value = get(i)
    if (value > peak) peak = value
  }
  return peak
}

/** The tallest entry of `start`–`end` and its colour. */
export function readRgbColumn(
  heights: Uint8Array,
  colors: Uint8Array,
  count: number,
  start: number,
  end: number
): { amp: number; color: number } | null {
  const span = columnSpan(count, start, end)
  if (!span) return null
  if (span.kind === 'hold') return { amp: heights[span.index] / 255, color: packColor(colors, span.index) }
  let peak = -1
  let peakIndex = span.from
  for (let i = span.from; i < span.to; i++) {
    if (heights[i] > peak) {
      peak = heights[i]
      peakIndex = i
    }
  }
  return { amp: Math.max(0, peak) / 255, color: packColor(colors, peakIndex) }
}

/** Fills the columns as shapes, one per run of the same colour. */
export function paintColumns(
  ctx: CanvasRenderingContext2D,
  pw: number,
  ph: number,
  anchor: 'bottom' | 'center',
  colorAt: (x: number) => string,
  smooth = false
): void {
  const { amp, color } = columns
  const mid = ph / 2
  let x = 0
  while (x < pw) {
    if (Number.isNaN(amp[x]) || color[x] < 0) {
      x++
      continue
    }
    const fill = colorAt(x)
    let end = x + 1
    while (end < pw && color[end] === color[x] && !Number.isNaN(amp[end])) end++
    ctx.fillStyle = fill
    const x0 = x
    const right = end < pw ? end + 0.35 : end
    if (anchor === 'center' && smooth) {
      ctx.beginPath()
      ctx.moveTo(x0, mid - amp[x0])
      for (let i = x0; i < end; i++) ctx.lineTo(i + 0.5, mid - amp[i])
      ctx.lineTo(right, mid - amp[end - 1])
      ctx.lineTo(right, mid + amp[end - 1])
      for (let i = end - 1; i >= x0; i--) ctx.lineTo(i + 0.5, mid + amp[i])
      ctx.closePath()
    } else if (anchor === 'center') {
      ctx.beginPath()
      ctx.moveTo(x0, mid - amp[x0])
      for (let i = x0; i < end - 1; i++) {
        ctx.lineTo(i + 1, mid - amp[i])
        ctx.lineTo(i + 1, mid - amp[i + 1])
      }
      ctx.lineTo(right, mid - amp[end - 1])
      ctx.lineTo(right, mid + amp[end - 1])
      for (let i = end - 1; i > x0; i--) {
        ctx.lineTo(i, mid + amp[i])
        ctx.lineTo(i, mid + amp[i - 1])
      }
      ctx.lineTo(x0, mid + amp[x0])
      ctx.closePath()
    } else {
      ctx.beginPath()
      ctx.moveTo(x0, ph)
      ctx.lineTo(x0, ph - amp[x0])
      for (let i = x0; i < end - 1; i++) {
        ctx.lineTo(i + 1, ph - amp[i])
        ctx.lineTo(i + 1, ph - amp[i + 1])
      }
      ctx.lineTo(right, ph - amp[end - 1])
      ctx.lineTo(right, ph)
      ctx.closePath()
    }
    ctx.fill()
    x = end
  }
}
