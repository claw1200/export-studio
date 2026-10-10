import type { DuplicateMatch, DurationSlack, Track } from '../types'

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

/** Case, punctuation, and extra whitespace. Mix names stay, so an original mix and a dub stay apart. */
export function normalizeMatchText(value: string): string {
  return value
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .replace(/ +/g, ' ')
    .trim()
}

/**
 * Non-overlapping duration buckets of `slack + 1` seconds.
 * Tracks inside one bucket differ by at most `slack`. A pair that sits on the
 * boundary can differ by less than that and still land in neighbouring buckets.
 */
function durationBucket(seconds: number, slack: number): number {
  if (slack <= 0) return seconds
  return Math.floor(seconds / (slack + 1))
}

function matchKey(track: Track, match: DuplicateMatch, durationSlack: number): string | null {
  const parts: string[] = []
  if (match.title) {
    const title = normalizeMatchText(track.title)
    if (!title) return null
    parts.push(title)
  }
  if (match.artist) parts.push(normalizeMatchText(track.artist))
  if (match.fileSize) {
    if (!(track.fileSize > 0)) return null
    parts.push(String(track.fileSize))
  }
  if (match.duration) parts.push(String(durationBucket(track.duration, durationSlack)))
  return parts.length ? parts.join('\u0000') : null
}

/** Groups of two or more tracks that share every enabled field, ordered by that key. */
export function duplicateGroups(tracks: readonly Track[], match: DuplicateMatch, durationSlack: number): Track[][] {
  if (!match.title && !match.artist && !match.duration && !match.fileSize) return []
  const buckets = new Map<string, Track[]>()
  for (const track of tracks) {
    const key = matchKey(track, match, durationSlack)
    if (key == null) continue
    const group = buckets.get(key)
    if (group) group.push(track)
    else buckets.set(key, [track])
  }
  return [...buckets.entries()]
    .filter(([, group]) => group.length >= 2)
    .sort(([a], [b]) => collator.compare(a, b))
    .map(([, group]) => group)
}

/** Prefer the copy that is higher quality, analysed, cued, and already in a playlist. */
export function rankCopies(group: readonly Track[], inPlaylist: ReadonlySet<string>): Track[] {
  return [...group].sort(
    (a, b) =>
      (b.bitrate || 0) - (a.bitrate || 0) ||
      Number(b.analysed) - Number(a.analysed) ||
      (b.cueCount || 0) - (a.cueCount || 0) ||
      Number(inPlaylist.has(b.id)) - Number(inPlaylist.has(a.id)) ||
      (b.fileSize || 0) - (a.fileSize || 0) ||
      collator.compare(a.path, b.path)
  )
}

export function isDurationSlack(value: unknown): value is DurationSlack {
  return value === 0 || value === 1 || value === 2 || value === 5
}

export function emptyDuplicateMatch(): DuplicateMatch {
  return { title: true, artist: true, duration: true, fileSize: false }
}
