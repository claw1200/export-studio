import type { Track } from '../types'

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

function compare(a: Track, b: Track, key: Exclude<SortKey, 'position'>): number {
  switch (key) {
    case 'bpm':
    case 'duration':
    case 'rating':
    case 'playCount':
    case 'year':
    case 'bitrate':
    case 'color':
      return a[key] - b[key]
    case 'key':
      return camelotRank(a.key) - camelotRank(b.key)
    default: {
      const av = a[key]
      const bv = b[key]
      if (!av && bv) return 1
      if (av && !bv) return -1
      return collator.compare(av, bv)
    }
  }
}

/** Sorts by `sort`, ties broken by title; `positions` orders by playlist position. */
export function sortTracks(list: Track[], sort: TrackSort, positions: ReadonlyMap<string, number>): Track[] {
  const { key, dir } = sort
  const order =
    key === 'position'
      ? (a: Track, b: Track) => (positions.get(a.id) ?? 0) - (positions.get(b.id) ?? 0)
      : (a: Track, b: Track) => compare(a, b, key)
  return [...list].sort((a, b) => order(a, b) * dir || collator.compare(a.title, b.title))
}

export function newestFirst(list: Track[]): Track[] {
  return [...list].sort((a, b) => collator.compare(b.dateAdded, a.dateAdded))
}
