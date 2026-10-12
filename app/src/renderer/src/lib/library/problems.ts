import type { ProblemChecks, ProblemCode, Track, TrackIssue } from '../types'

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

const EXTENSIONS: Record<string, string> = {
  mp3: 'MP3',
  m4a: 'M4A',
  mp4: 'M4A',
  flac: 'FLAC',
  wav: 'WAV',
  aif: 'AIFF',
  aiff: 'AIFF'
}

const SEVERITY: Record<ProblemCode, number> = {
  missing: 0,
  unreadable: 1,
  type: 2,
  incompatible: 3,
  size: 4,
  duration: 5
}

const CODES: ProblemCode[] = ['missing', 'unreadable', 'type', 'incompatible', 'size', 'duration']

export function emptyProblemChecks(): ProblemChecks {
  return {
    missing: true,
    unreadable: true,
    type: true,
    incompatible: true,
    size: true,
    duration: true
  }
}

export function isProblemChecks(value: unknown): value is ProblemChecks {
  if (!value || typeof value !== 'object') return false
  const checks = value as ProblemChecks
  return CODES.every((code) => typeof checks[code] === 'boolean')
}

export function extensionType(path: string): string | null {
  const file = path.replaceAll('\\', '/').split('/').pop() ?? ''
  const dot = file.lastIndexOf('.')
  if (dot <= 0) return null
  return EXTENSIONS[file.slice(dot + 1).toLowerCase()] ?? null
}

/** Missing files and an extension that disagrees with rekordbox. Header checks come from the engine. */
export function localIssues(track: Track): TrackIssue[] {
  if (track.streaming) return []
  const issues: TrackIssue[] = []
  if (track.missing || !track.path) issues.push({ code: 'missing', detail: 'File missing' })
  const ext = extensionType(track.path)
  if (track.storedType && ext && track.storedType !== ext) {
    issues.push({ code: 'type', detail: `Rekordbox says ${track.storedType}, the file is ${ext}` })
  }
  return issues
}

export function shownIssues(issues: readonly TrackIssue[], checks: ProblemChecks): TrackIssue[] {
  return issues.filter((issue) => checks[issue.code]).sort((a, b) => SEVERITY[a.code] - SEVERITY[b.code])
}

export function issueText(issues: readonly TrackIssue[]): string {
  return issues.map((issue) => issue.detail).join(' · ')
}

function problemRank(issues: readonly TrackIssue[]): number {
  return issues.reduce((rank, issue) => Math.min(rank, SEVERITY[issue.code]), 9)
}

/** Collection tracks that fail a turned-on check, severe problems first. */
export function problemRows(
  tracks: readonly Track[],
  checks: ProblemChecks,
  probed: Readonly<Record<string, TrackIssue[]>>
): { track: Track; text: string }[] {
  const rows: { track: Track; text: string; rank: number }[] = []
  for (const track of tracks) {
    if (track.streaming) continue
    const issues = Object.prototype.hasOwnProperty.call(probed, track.id) ? probed[track.id] : localIssues(track)
    const shown = shownIssues(issues, checks)
    if (!shown.length) continue
    rows.push({ track, text: issueText(shown), rank: problemRank(shown) })
  }
  rows.sort(
    (a, b) =>
      a.rank - b.rank || collator.compare(a.track.title, b.track.title) || collator.compare(a.track.id, b.track.id)
  )
  return rows.map(({ track, text }) => ({ track, text }))
}
