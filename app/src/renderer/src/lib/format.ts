export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00'
  const s = Math.floor(seconds)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = String(s % 60).padStart(2, '0')
  return h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`
}

export function formatClock(ms: number): string {
  const total = Math.max(0, ms) / 1000
  const m = Math.floor(total / 60)
  const s = Math.floor(total % 60)
  const cs = Math.floor((total * 100) % 100)
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}.${String(cs).padStart(2, '0')}`
}

export function formatTotal(seconds: number): string {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h ? `${h} h ${m} min` : `${m} min`
}

export function formatBpm(bpm: number): string {
  return bpm ? bpm.toFixed(bpm % 1 ? 2 : 0) : ''
}

export function formatSize(bytes: number): string {
  if (!bytes) return ''
  const mb = bytes / 1024 / 1024
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`
}

/** rekordbox track colour IDs 1–8 */
export const TRACK_COLORS: Record<number, { name: string; hex: string }> = {
  1: { name: 'Pink', hex: '#f472b6' },
  2: { name: 'Red', hex: '#ef4444' },
  3: { name: 'Orange', hex: '#f97316' },
  4: { name: 'Yellow', hex: '#eab308' },
  5: { name: 'Green', hex: '#22c55e' },
  6: { name: 'Aqua', hex: '#06b6d4' },
  7: { name: 'Blue', hex: '#3b82f6' },
  8: { name: 'Purple', hex: '#a855f7' }
}

const CAMELOT_HUES = [170, 140, 100, 60, 30, 10, 345, 320, 290, 260, 230, 200]

/** Colour-wheel hue for a Camelot / Open Key / musical key string. */
export function keyColor(key: string): string | null {
  const m = /^(\d{1,2})([AB])$/i.exec(key.trim())
  if (!m) return null
  const n = Number(m[1])
  if (n < 1 || n > 12) return null
  const minor = m[2].toUpperCase() === 'A'
  return `oklch(${minor ? 0.72 : 0.8} 0.14 ${CAMELOT_HUES[n - 1]})`
}

/** djmdCue.Kind → hot cue letter. rekordbox skips Kind 4. */
export function hotCueLetter(kind: number): string {
  const index = kind >= 5 ? kind - 2 : kind - 1
  return 'ABCDEFGH'[index] ?? '?'
}

/** djmdCue.Kind for pads A–H. */
export const HOT_CUE_KINDS = [1, 2, 3, 5, 6, 7, 8, 9]

/**
 * rekordbox hot cue palette. djmdCue.ColorTableIndex `n` is entry `n - 1`; 0 and null mean no colour.
 * Table from Deep Symmetry's beat-link (CueList.findRekordboxColor).
 */
// prettier-ignore
const HOT_CUE_PALETTE = [
  '#305aff', '#5073ff', '#508cff', '#50a0ff', '#50b4ff', '#50b0f2', '#50aee8', '#45acdb',
  '#00e0ff', '#19daf0', '#32d2e6', '#21b4b9', '#20aaa0', '#1fa392', '#19a08c', '#14a584',
  '#14aa7d', '#10b176', '#30d26e', '#37de5a', '#3ceb50', '#28e214', '#7dc13d', '#8cc832',
  '#9bd723', '#a5e116', '#a5dc0a', '#aad208', '#b4c805', '#b4be04', '#bab404', '#c3af04',
  '#e1aa00', '#ffa000', '#ff9600', '#ff8c00', '#ff7500', '#e0641b', '#e0461e', '#e0301e',
  '#e02823', '#e62828', '#ff376f', '#ff2d6f', '#ff127b', '#f51e8c', '#eb2da0', '#e637b4',
  '#de44cf', '#de448d', '#e630b4', '#e619dc', '#e600ff', '#dc00ff', '#cc00ff', '#b432ff',
  '#b93cff', '#c542ff', '#aa5aff', '#aa72ff', '#8272ff', '#6473ff'
]

/** The 16 palette entries rekordbox offers in its hot cue colour picker, as ColorTableIndex values. */
export const HOT_CUE_PICKER = [1, 5, 9, 14, 18, 22, 26, 30, 32, 38, 42, 45, 49, 56, 60, 62]

export function hotCuePaletteColor(index: number | null): string | null {
  return index ? (HOT_CUE_PALETTE[index - 1] ?? null) : null
}

export const HOT_CUE_DEFAULT = '#2be36c'
export const LOOP_DEFAULT = '#ff9a1f'
export const MEMORY_CUE_DEFAULT = '#ff3b4e'
