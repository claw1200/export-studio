import { emptyDuplicateMatch } from './duplicates'
import { emptyProblemChecks } from './problems'
import type { TrackViewFilters, View } from '../types'

/** Built-in views that existed before Damaged tracks. A deleted copy of one of these stays deleted. */
export const LEGACY_VIEW_IDS = ['default:not-in-playlist', 'default:music', 'default:duplicates']

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
  views.push({
    id: 'default:problems',
    name: 'Damaged tracks',
    type: 'problems',
    checks: emptyProblemChecks()
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

/**
 * Offers each built-in once. Views introduced before `introduced` was stored count as already
 * offered, so deleting Suspected duplicates and then updating does not bring it back.
 */
export function introduceViews(
  stored: View[],
  musicDirectory: string,
  alreadySeeded: boolean,
  introduced: readonly string[]
): { views: View[]; introduced: string[] } {
  const defaults = defaultViews(musicDirectory)
  if (!alreadySeeded) {
    return {
      views: seedStoredViews(stored, musicDirectory, false),
      introduced: [...new Set([...introduced, ...LEGACY_VIEW_IDS, ...defaults.map((view) => view.id)])]
    }
  }
  const known = new Set(introduced.length ? introduced : LEGACY_VIEW_IDS)
  const present = new Set(stored.map((view) => view.id))
  const added = defaults.filter((view) => !known.has(view.id) && !present.has(view.id))
  return {
    views: added.length ? [...added, ...stored] : stored,
    introduced: [...new Set([...known, ...defaults.map((view) => view.id)])]
  }
}
