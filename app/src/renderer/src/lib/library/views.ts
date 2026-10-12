import { formatDuration } from '$lib/format'
import { isDurationSlack } from '$lib/library/duplicates'
import { isProblemChecks } from '$lib/library/problems'
import { getSetting, setSetting } from '$lib/settings'
import type { DuplicateMatch, PlaylistMembership, Track, TrackViewFilters, View } from '$lib/types'
import { introduceViews } from './defaults'

export { emptyFilters } from './defaults'

const STORAGE_KEY = 'views'
const SEEDED_KEY = 'viewsSeeded'
const INTRODUCED_KEY = 'viewsIntroduced'

export interface FileEntry {
  path: string
  /** Tag metadata for a file that is not in the collection. Null until it has been read. */
  loose: Track | null
}

export function pathKey(path: string): string {
  const stored = path.replaceAll('\\', '/')
  const platform = window.api.platform
  return platform === 'darwin' || platform === 'win32' ? stored.toLowerCase() : stored
}

export function looseTrack(path: string, row: Partial<Track> = {}): Track {
  const stored = (row.path || path).replaceAll('\\', '/')
  const file = stored.split('/').pop() ?? stored
  const dot = file.lastIndexOf('.')
  const stem = dot > 0 ? file.slice(0, dot) : file
  const suffix = dot > 0 ? file.slice(dot + 1).toUpperCase() : ''
  return {
    id: `file:${encodeURIComponent(stored)}`,
    title: row.title || stem || file,
    artist: row.artist ?? '',
    album: row.album ?? '',
    genre: row.genre ?? '',
    label: row.label ?? '',
    remixer: row.remixer ?? '',
    originalArtist: row.originalArtist ?? '',
    composer: row.composer ?? '',
    key: row.key ?? '',
    bpm: row.bpm ?? 0,
    duration: row.duration ?? 0,
    rating: row.rating ?? 0,
    color: row.color ?? 0,
    comment: row.comment ?? '',
    year: row.year ?? 0,
    dateAdded: row.dateAdded ?? '',
    dateCreated: row.dateCreated ?? '',
    playCount: row.playCount ?? 0,
    bitrate: row.bitrate ?? 0,
    sampleRate: row.sampleRate ?? 0,
    bitDepth: row.bitDepth ?? 0,
    fileType: row.fileType || (suffix === 'AIF' ? 'AIFF' : suffix),
    storedType: row.storedType ?? null,
    path: stored,
    missing: row.missing ?? false,
    streaming: false,
    fileSize: row.fileSize ?? 0,
    hasArtwork: false,
    trackNo: row.trackNo ?? 0,
    analysed: false,
    myTags: [],
    cueStars: 0,
    cueCount: 0,
    phrase: false,
    collected: false
  }
}

function contains(value: string, query: string): boolean {
  const needle = query.trim().toLowerCase()
  return !needle || value.toLowerCase().includes(needle)
}

/** Every filled-in criterion must match. A BPM range excludes tracks that have no BPM. */
export function matchesTrack(track: Track, filters: TrackViewFilters, inPlaylist: boolean): boolean {
  if (!contains(track.title, filters.title)) return false
  if (!contains(track.artist, filters.artist)) return false
  if (!contains(track.album, filters.album)) return false
  if (!contains(track.genre, filters.genre)) return false
  if (!contains(track.label, filters.label)) return false
  if (!contains(track.comment, filters.comment)) return false
  if (filters.bpmMin != null || filters.bpmMax != null) {
    if (!(track.bpm > 0)) return false
    if (filters.bpmMin != null && track.bpm < filters.bpmMin) return false
    if (filters.bpmMax != null && track.bpm > filters.bpmMax) return false
  }
  if (filters.durationMin != null && track.duration < filters.durationMin) return false
  if (filters.durationMax != null && track.duration > filters.durationMax) return false
  if (filters.keys.length) {
    const key = track.key.trim().toLowerCase()
    if (!filters.keys.some((item) => item.trim().toLowerCase() === key)) return false
  }
  if (filters.playlist === 'in' && !inPlaylist) return false
  if (filters.playlist === 'out' && inPlaylist) return false
  return true
}

export function formatDurationInput(seconds: number | null): string {
  return seconds == null ? '' : formatDuration(seconds)
}

/** Blank is no bound. A number is seconds. `m:ss` and `h:mm:ss` are durations. Invalid text returns undefined. */
export function parseDurationInput(text: string): number | null | undefined {
  const trimmed = text.trim()
  if (!trimmed) return null
  const parts = trimmed.split(':')
  if (parts.some((part) => part === '' || !/^\d+$/.test(part))) return undefined
  const numbers = parts.map((part) => Number(part))
  if (numbers.length === 1) return numbers[0]
  if (numbers.length === 2) {
    const [minutes, seconds] = numbers
    if (seconds >= 60) return undefined
    return minutes * 60 + seconds
  }
  if (numbers.length === 3) {
    const [hours, minutes, seconds] = numbers
    if (minutes >= 60 || seconds >= 60) return undefined
    return hours * 3600 + minutes * 60 + seconds
  }
  return undefined
}

function isNumber(value: unknown): value is number | null {
  return value === null || (typeof value === 'number' && Number.isFinite(value))
}

function isMembership(value: unknown): value is PlaylistMembership {
  return value === 'any' || value === 'in' || value === 'out'
}

function isFilters(value: unknown): value is TrackViewFilters {
  if (!value || typeof value !== 'object') return false
  const filters = value as TrackViewFilters
  return (
    typeof filters.title === 'string' &&
    typeof filters.artist === 'string' &&
    typeof filters.album === 'string' &&
    typeof filters.genre === 'string' &&
    typeof filters.label === 'string' &&
    typeof filters.comment === 'string' &&
    isNumber(filters.bpmMin) &&
    isNumber(filters.bpmMax) &&
    isNumber(filters.durationMin) &&
    isNumber(filters.durationMax) &&
    Array.isArray(filters.keys) &&
    filters.keys.every((key) => typeof key === 'string') &&
    isMembership(filters.playlist)
  )
}

function isDuplicateMatch(value: unknown): value is DuplicateMatch {
  if (!value || typeof value !== 'object') return false
  const match = value as DuplicateMatch
  return (
    typeof match.title === 'boolean' &&
    typeof match.artist === 'boolean' &&
    typeof match.duration === 'boolean' &&
    typeof match.fileSize === 'boolean'
  )
}

function isView(value: unknown): value is View {
  if (!value || typeof value !== 'object') return false
  const view = value as View
  if (typeof view.id !== 'string' || !view.id || typeof view.name !== 'string') return false
  if (view.type === 'tracks') return isFilters(view.filters)
  if (view.type === 'files') return typeof view.directory === 'string' && typeof view.showInCollection === 'boolean'
  if (view.type === 'duplicates') return isDuplicateMatch(view.match) && isDurationSlack(view.durationSlack)
  if (view.type === 'problems') return isProblemChecks(view.checks)
  return false
}

function readStoredViews(): View[] {
  try {
    const raw = JSON.parse(getSetting(STORAGE_KEY) ?? '[]') as unknown
    return Array.isArray(raw) ? raw.filter(isView) : []
  } catch {
    return []
  }
}

function readIntroduced(): string[] {
  try {
    const raw = JSON.parse(getSetting(INTRODUCED_KEY) ?? '[]') as unknown
    return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

export function loadViews(): View[] {
  const stored = readStoredViews()
  const alreadySeeded = getSetting(SEEDED_KEY) === '1'
  const music = window.api.musicDirectory
  if (!alreadySeeded && typeof music !== 'string') return stored
  const seeded = introduceViews(stored, music ?? '', alreadySeeded, readIntroduced())
  if (!alreadySeeded) setSetting(SEEDED_KEY, '1')
  setSetting(INTRODUCED_KEY, JSON.stringify(seeded.introduced))
  if (seeded.views !== stored) saveViews(seeded.views)
  return seeded.views
}

export function saveViews(views: View[]): void {
  setSetting(STORAGE_KEY, JSON.stringify(views))
}
