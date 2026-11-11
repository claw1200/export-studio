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

export function packRgb(red: number, green: number, blue: number): number {
  return (Math.round(red) << 16) | (Math.round(green) << 8) | Math.round(blue)
}

export function packColor(colors: Uint8Array, index: number): number {
  const o = index * 3
  return packRgb(colors[o], colors[o + 1], colors[o + 2])
}

export function cssColor(packed: number): string {
  return `rgb(${(packed >> 16) & 255},${(packed >> 8) & 255},${packed & 255})`
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

const BYTE_MAX = 255

/** A waveform byte as a level from 0 to 1. */
export function byteLevel(byte: number): number {
  return byte / BYTE_MAX
}

/*
 * Reading a pixel column that covers entries `start`–`end`. A column narrower than one entry shows
 * the entry under its middle; a wider one shows the peak of the entries it touches.
 *
 * These run for every pixel column of every deck frame, so they take and return plain numbers:
 * an object or closure per column would be thousands of allocations a frame.
 */

function coversNoEntries(count: number, start: number, end: number): boolean {
  return !(end > start) || end <= 0 || start >= count
}

function isWithinOneEntry(start: number, end: number): boolean {
  return end - start <= 1
}

/** The fractional entry under the column's middle, kept inside the data. */
function entryUnderMiddle(count: number, start: number, end: number): number {
  return Math.min(count - 1, Math.max(0, (start + end) / 2))
}

function firstTouchedEntry(start: number): number {
  return Math.max(0, Math.floor(start))
}

function pastLastTouchedEntry(count: number, end: number): number {
  return Math.min(count, Math.ceil(end))
}

/** Slopes down toward the next entry. A rise stays flat until the entry boundary. */
function fallToward(current: number, next: number, fraction: number): number {
  return next >= current ? current : current + (next - current) * fraction
}

/** Interleaved low/mid/high band bytes, read every `stride` raw entries as one. */
export interface BandSource {
  data: Uint8Array
  /** Entries in `data`. */
  rawCount: number
  stride: number
  /** Entries after `stride`; what `start` and `end` index. */
  count: number
}

export function bandSource(data: Uint8Array, stride: number): BandSource {
  const rawCount = data.length / 3
  return { data, rawCount, stride, count: logicalCount(rawCount, stride) }
}

function bandByte(source: BandSource, band: number, entry: number): number {
  const { data, rawCount, stride } = source
  return stride <= 1 ? data[entry * 3 + band] : stridePeak(data, rawCount, entry, stride, band)
}

/**
 * One band's level, 0 to 1, for the column covering `start`–`end`, or NaN outside the data.
 * With `fall`, a column narrower than an entry slopes down toward the next one.
 */
export function readBandLevel(source: BandSource, band: number, start: number, end: number, fall: boolean): number {
  const { count } = source
  if (coversNoEntries(count, start, end)) return NaN
  if (isWithinOneEntry(start, end)) {
    const at = entryUnderMiddle(count, start, end)
    const entry = at | 0
    const current = byteLevel(bandByte(source, band, entry))
    if (!fall) return current
    const next = byteLevel(bandByte(source, band, Math.min(count - 1, entry + 1)))
    return fallToward(current, next, at - entry)
  }
  const to = pastLastTouchedEntry(count, end)
  let peak = 0
  for (let entry = firstTouchedEntry(start); entry < to; entry++) peak = Math.max(peak, bandByte(source, band, entry))
  return byteLevel(peak)
}

/** The tallest entry the column covering `start`–`end` touches (the first, on a tie), or -1 outside the data. */
export function rgbPeakIndex(heights: Uint8Array, count: number, start: number, end: number): number {
  if (coversNoEntries(count, start, end)) return -1
  if (isWithinOneEntry(start, end)) return entryUnderMiddle(count, start, end) | 0
  const from = firstTouchedEntry(start)
  const to = pastLastTouchedEntry(count, end)
  let tallest = from
  for (let entry = from + 1; entry < to; entry++) if (heights[entry] > heights[tallest]) tallest = entry
  return tallest
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
