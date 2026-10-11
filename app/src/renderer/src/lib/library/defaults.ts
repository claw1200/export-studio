import { emptyDuplicateMatch } from './duplicates'
import type { TrackViewFilters, View } from '../types'

export function emptyFilters(): TrackViewFilters {
  return {
    title: '',
    artist: '',
    album: '',
    genre: '',
    label: '',
    comment: '',
    bpmMin: null,
    bpmMax: null,
    durationMin: null,
    durationMax: null,
    keys: [],
    playlist: 'any'
  }
}

/** Built-in views. Stable ids so seeding does not add a second copy. */
export function defaultViews(musicDirectory: string): View[] {
  const views: View[] = [
    {
      id: 'default:not-in-playlist',
      name: 'Not in a playlist',
      type: 'tracks',
      filters: { ...emptyFilters(), playlist: 'out' }
    }
  ]
  const directory = musicDirectory.trim().replaceAll('\\', '/')
  if (directory) {
    views.push({
      id: 'default:music',
      name: 'Music folder',
      type: 'files',
      directory,
      showInCollection: true
    })
  }
  views.push({
    id: 'default:duplicates',
    name: 'Suspected duplicates',
    type: 'duplicates',
    match: emptyDuplicateMatch(),
    durationSlack: 2
  })
  return views
}

/** Adds the built-in views the first time. Deleting one afterwards leaves it gone. */
export function seedStoredViews(stored: View[], musicDirectory: string, alreadySeeded: boolean): View[] {
  if (alreadySeeded) return stored
  const ids = new Set(stored.map((view) => view.id))
  const added = defaultViews(musicDirectory).filter((view) => !ids.has(view.id))
  return added.length ? [...added, ...stored] : stored
}
