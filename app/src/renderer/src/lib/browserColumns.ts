import type { SortKey } from './library/sort'

export type Column = {
  id: string
  name: string
  label: string
  width: number
  min: number
  max: number
  /** Width the cell content needs, in em of the row text. The column can't be narrower than this plus padding. */
  fit?: number
  fr?: number
  align?: 'right' | 'center'
  sort?: SortKey
}

/** One whole cue star plus the visible edge of the two it overlaps when the column is narrow. */
const CUE_FIT = 1.12 + 2 * 0.36

export const COLUMNS: Column[] = [
  { id: 'index', name: '#', label: '#', width: 44, min: 36, max: 72, align: 'right', sort: 'position' },
  { id: 'color', name: 'Colour', label: 'Colour', width: 80, min: 20, max: 160, align: 'center', sort: 'color' },
  { id: 'art', name: 'Artwork', label: 'Artwork', width: 88, min: 32, max: 180 },
  { id: 'title', name: 'Title', label: 'Title', width: 220, min: 120, max: 640, fr: 2.2, sort: 'title' },
  { id: 'artist', name: 'Artist', label: 'Artist', width: 150, min: 80, max: 480, fr: 1.4, sort: 'artist' },
  { id: 'album', name: 'Album', label: 'Album', width: 120, min: 72, max: 480, fr: 1, sort: 'album' },
  { id: 'genre', name: 'Genre', label: 'Genre', width: 120, min: 56, max: 320, sort: 'genre' },
  { id: 'bpm', name: 'BPM', label: 'BPM', width: 64, min: 48, max: 120, align: 'right', sort: 'bpm' },
  { id: 'key', name: 'Key', label: 'Key', width: 52, min: 40, max: 120, align: 'center', sort: 'key' },
  { id: 'duration', name: 'Time', label: 'Time', width: 60, min: 48, max: 120, align: 'right', sort: 'duration' },
  {
    id: 'compat',
    name: 'Hardware',
    label: 'Hardware',
    // Just wide enough for the heading and its sort arrow.
    width: 84,
    min: 24,
    max: 180,
    fit: 0.8,
    align: 'center',
    sort: 'compat'
  },
  { id: 'wave', name: 'Waveform', label: 'Waveform', width: 184, min: 80, max: 480, sort: 'cueCount' },
  {
    id: 'cue',
    name: 'CUE',
    label: 'CUE',
    width: 72,
    min: 24,
    max: 110,
    fit: CUE_FIT,
    align: 'center',
    sort: 'cueStars'
  },
  { id: 'rating', name: 'Rating', label: 'Rating', width: 78, min: 64, max: 140, sort: 'rating' },
  { id: 'dateAdded', name: 'Added', label: 'Added', width: 92, min: 64, max: 180, sort: 'dateAdded' },
  { id: 'fileType', name: 'Type', label: 'Type', width: 58, min: 44, max: 120, sort: 'fileType' }
]

export const BROWSER_COLUMNS_KEY = 'browserColumns'
export const COLUMN_LAYOUT_VERSION = 3

/** Horizontal padding of a browser cell, in pixels. */
export const CELL_PADDING = 16

/** The narrowest a column can be at a row text size: its content, not its heading, sets the limit. */
export function minColumnWidth(column: Column, fontSize: number): number {
  if (!column.fit) return column.min
  return Math.min(column.max, Math.max(column.min, Math.ceil(column.fit * fontSize) + CELL_PADDING))
}

export const BROWSER_ROW_HEIGHT_KEY = 'browserRowHeight'
/** Track row height in the browser, in pixels. The default is the height rows had before it could change. */
export const ROW_HEIGHT_DEFAULT = 32
export const ROW_HEIGHT_MIN = 24
export const ROW_HEIGHT_MAX = 56
export const ROW_HEIGHT_STEP = 2
/** Row text size at the default height; everything in a row is sized in em from it. */
const ROW_FONT_DEFAULT = 12.5
const ROW_FONT_MIN = 10

/** Rounds a row height to the slider step and keeps it inside the range. */
export function clampRowHeight(height: number): number {
  if (!Number.isFinite(height)) return ROW_HEIGHT_DEFAULT
  const stepped = Math.round((height - ROW_HEIGHT_MIN) / ROW_HEIGHT_STEP) * ROW_HEIGHT_STEP + ROW_HEIGHT_MIN
  return Math.min(ROW_HEIGHT_MAX, Math.max(ROW_HEIGHT_MIN, stepped))
}

/** Rows in the alternative track style are taller than the chosen height, to fit two lines and a bigger cover. */
export const STACKED_ROW_SCALE = 1.5

/** Row height for the alternative track style. Text keeps the size of the chosen height. */
export function stackedRowHeight(height: number): number {
  return Math.round(clampRowHeight(height) * STACKED_ROW_SCALE)
}

/** The stored row height, or the default when nothing usable is stored. */
export function readRowHeight(raw: string | null): number {
  if (raw == null || raw.trim() === '') return ROW_HEIGHT_DEFAULT
  return clampRowHeight(Number(raw))
}

/** Row text size for a row height, in pixels. It grows with the row and never drops below a readable size. */
export function rowFontSize(height: number): number {
  const size = (ROW_FONT_DEFAULT * clampRowHeight(height)) / ROW_HEIGHT_DEFAULT
  return Math.max(ROW_FONT_MIN, Math.round(size * 4) / 4)
}

const SHOWN_BY_DEFAULT = [
  'index',
  'compat',
  'cue',
  'wave',
  'art',
  'title',
  'artist',
  'bpm',
  'key',
  'duration',
  'dateAdded'
]

export type ColumnPrefs = {
  widths: Record<string, number>
  sized: string[]
  hidden: string[]
  order: string[]
}

export function defaultOrder(): string[] {
  const order = SHOWN_BY_DEFAULT.filter((id) => COLUMNS.some((column) => column.id === id))
  for (const column of COLUMNS) if (!order.includes(column.id)) order.push(column.id)
  return order
}

function placeAfter(ids: string[], id: string, after: string): string[] {
  const next = ids.filter((item) => item !== id)
  const at = next.indexOf(after)
  next.splice(at + 1, 0, id)
  return next
}

export function defaultHidden(): string[] {
  return COLUMNS.map((column) => column.id).filter((id) => !SHOWN_BY_DEFAULT.includes(id))
}

export function readColumnPrefs(raw: string | null): ColumnPrefs {
  const widths = Object.fromEntries(COLUMNS.map((column) => [column.id, column.width]))
  let sized: string[] = []
  let hidden = defaultHidden()
  let order = defaultOrder()
  try {
    if (raw) {
      const prefs = JSON.parse(raw) as {
        layout?: unknown
        widths?: Record<string, unknown>
        sized?: unknown
        hidden?: unknown
        order?: unknown
      }
      const savedLayout = prefs.layout === 2 || prefs.layout === COLUMN_LAYOUT_VERSION
      if (prefs.widths && typeof prefs.widths === 'object') {
        for (const column of COLUMNS) {
          const width = prefs.widths[column.id]
          if (typeof width === 'number' && Number.isFinite(width)) {
            widths[column.id] = Math.min(column.max, Math.max(column.min, Math.round(width)))
          }
        }
      }
      if (Array.isArray(prefs.sized)) {
        sized = prefs.sized.filter(
          (id): id is string => typeof id === 'string' && COLUMNS.some((column) => column.id === id && column.fr)
        )
      }
      if (savedLayout && Array.isArray(prefs.hidden)) {
        hidden = prefs.hidden.filter((id): id is string => typeof id === 'string' && widths[id] !== undefined)
        if (hidden.length >= COLUMNS.length) hidden = []
      }
      if (savedLayout && Array.isArray(prefs.order)) {
        const seen: Record<string, true> = {}
        const next: string[] = []
        for (const id of prefs.order) {
          if (typeof id === 'string' && widths[id] !== undefined && !seen[id]) {
            next.push(id)
            seen[id] = true
          }
        }
        for (const id of order) if (!seen[id]) next.push(id)
        order = prefs.layout === COLUMN_LAYOUT_VERSION ? next : placeAfter(next, 'compat', 'index')
      }
    }
  } catch {
    return { widths, sized, hidden, order }
  }
  return { widths, sized, hidden, order }
}
