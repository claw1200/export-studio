import { HOT_CUE_KINDS } from '../format'
import type { Cue, Track } from '../types'

export type SortKey =
  | 'title'
  | 'artist'
  | 'album'
  | 'genre'
  | 'label'
  | 'bpm'
  | 'key'
  | 'duration'
  | 'rating'
  | 'color'
  | 'dateAdded'
  | 'playCount'
  | 'year'
  | 'bitrate'
  | 'fileType'
  | 'comment'
  | 'remixer'
  | 'position'
  | 'cueStars'
  | 'cueCount'
  | 'compat'

export interface TrackSort {
  key: SortKey
  dir: 1 | -1
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

function camelotRank(key: string): number {
  const m = /^(\d{1,2})([AB])$/i.exec(key)
  if (!m) return key ? 1000 : 2000
  return Number(m[1]) * 2 + (m[2].toUpperCase() === 'B' ? 1 : 0)
}

function compare(a: Track, b: Track, key: Exclude<SortKey, 'position' | 'compat'>): number {
  switch (key) {
    case 'bpm':
    case 'duration':
    case 'rating':
    case 'playCount':
    case 'year':
    case 'bitrate':
    case 'color':
    case 'cueStars':
    case 'cueCount':
      return (a[key] ?? 0) - (b[key] ?? 0)
    case 'key':
      return camelotRank(a.key) - camelotRank(b.key)
    case 'dateAdded':
      return (
        collator.compare(a.dateAdded, b.dateAdded) ||
        collator.compare(a.addedAt ?? '', b.addedAt ?? '')
      )
    default: {
      const av = a[key]
      const bv = b[key]
      if (!av && bv) return 1
      if (av && !bv) return -1
      return collator.compare(av, bv)
    }
  }
}

/** Sorts by `sort`, ties broken by title. `positions` orders a playlist. `ranks` orders the hardware light. */
export function sortTracks(
  list: Track[],
  sort: TrackSort,
  positions: ReadonlyMap<string, number>,
  ranks?: ReadonlyMap<string, number>
): Track[] {
  const { key, dir } = sort
  const order =
    key === 'position'
      ? (a: Track, b: Track) => (positions.get(a.id) ?? 0) - (positions.get(b.id) ?? 0)
      : key === 'compat'
        ? (a: Track, b: Track) => (ranks?.get(a.id) ?? 0) - (ranks?.get(b.id) ?? 0)
        : (a: Track, b: Track) => compare(a, b, key)
  return [...list].sort((a, b) => order(a, b) * dir || collator.compare(a.title, b.title))
}

export function newestFirst(list: Track[]): Track[] {
  return [...list].sort(
    (a, b) =>
      collator.compare(b.dateAdded, a.dateAdded) || collator.compare(b.addedAt ?? '', a.addedAt ?? '')
  )
}

/** Star total and placed-cue total after a cue edit. Phrase analysis is unchanged. */
export function cueStats(cues: Cue[], phrase: boolean): { cueStars: number; cueCount: number } {
  let memory = 0
  let hot = 0
  for (const cue of cues) {
    if (cue.slot === 0) memory++
    else if (HOT_CUE_KINDS.includes(cue.slot)) hot++
  }
  return {
    cueCount: memory + hot,
    cueStars: (memory > 0 ? 1 : 0) + (hot > 0 ? 1 : 0) + (phrase ? 1 : 0)
  }
}
