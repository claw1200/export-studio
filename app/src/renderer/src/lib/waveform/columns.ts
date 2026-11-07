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

/*
 * The readers below run for every pixel column of every frame the deck draws, so they take plain
 * arguments and return numbers: no closure per call and no object per column, which would
 * otherwise be thousands of short-lived allocations a frame and steady garbage collection.
 *
 * A pixel column covers entries `start`–`end`. Narrower than one entry, it holds the entry under its
 * middle; wider, it takes the peak of the entries it touches.
 */

/** One band's byte at logical entry `i`, reading every `stride` entries as one. */
function bandByte(data: Uint8Array, src: number, i: number, stride: number, band: number): number {
  return stride <= 1 ? data[i * 3 + band] : stridePeak(data, src, i, stride, band)
}

/**
 * One band's level, 0 to 1, for the pixel column covering logical entries `start`–`end` of
 * interleaved low/mid/high `data` (`src` entries, `count` after `stride`), or NaN outside the data.
 * With `lerpDown`, a column narrower than an entry slopes down toward the next entry; a rise stays
 * flat until the entry boundary.
 */
export function readBandLevel(
  data: Uint8Array,
  src: number,
  count: number,
  stride: number,
  band: number,
  start: number,
  end: number,
  lerpDown: boolean
): number {
  if (!(end > start) || end <= 0 || start >= count) return NaN
  if (end - start <= 1) {
    const at = Math.min(count - 1, Math.max(0, (start + end) / 2))
    const index = at | 0
    const current = bandByte(data, src, index, stride, band) / 255
    if (!lerpDown) return current
    const next = bandByte(data, src, Math.min(count - 1, index + 1), stride, band) / 255
    return next >= current ? current : current + (next - current) * (at - index)
  }
  const to = Math.min(count, Math.ceil(end))
  let peak = 0
  for (let i = Math.max(0, Math.floor(start)); i < to; i++) {
    const value = bandByte(data, src, i, stride, band)
    if (value > peak) peak = value
  }
  return peak / 255
}

/** The tallest of entries `start`–`end` (the first, on a tie), or -1 outside the data. */
export function rgbPeakIndex(heights: Uint8Array, count: number, start: number, end: number): number {
  if (!(end > start) || end <= 0 || start >= count) return -1
  if (end - start <= 1) return Math.min(count - 1, Math.max(0, (start + end) / 2)) | 0
  const from = Math.max(0, Math.floor(start))
  const to = Math.min(count, Math.ceil(end))
  let peakIndex = from
  for (let i = from + 1; i < to; i++) if (heights[i] > heights[peakIndex]) peakIndex = i
  return peakIndex
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
