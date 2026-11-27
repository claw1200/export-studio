import type { SyncProgress } from './types'

const PHASE_LABELS: Record<string, string> = {
  format: 'Formatting the USB',
  reading: 'Reading the volume',
  writing: 'Writing the library',
  verifying: 'Checking the export',
  cleanup: 'Tidying up',
  done: 'Finalizing sync',
  eject: 'Ejecting device'
}

const TRACK_PHASES: Record<string, { verb: string; uncounted: string }> = {
  checking: { verb: 'Checking', uncounted: 'Checking the USB' },
  planning: { verb: 'Reading', uncounted: 'Reading changed tracks' },
  preparing: { verb: 'Preparing', uncounted: 'Preparing audio' },
  copy: { verb: 'Copying', uncounted: 'Copying audio' },
  convert: { verb: 'Converting', uncounted: 'Converting audio' },
  import: { verb: 'Importing', uncounted: 'Importing' }
}

function trackLabel({ done = 0, total = 0, name }: SyncProgress, verb: string, uncounted: string): string {
  if (!total) return uncounted
  return `${verb} ${Math.min(done + 1, total)} of ${total}${name ? ` · ${name}` : ''}`
}

function publishingLabel({ done = 0, total = 0 }: SyncProgress): string {
  if (!total) return 'Publishing'
  return `Publishing ${done.toLocaleString()} of ${total.toLocaleString()} files`
}

export function syncPhaseLabel(progress: SyncProgress | null): string {
  if (!progress) return 'Preparing sync'
  if (progress.phase === 'publishing') return publishingLabel(progress)
  const trackPhase = TRACK_PHASES[progress.phase]
  if (trackPhase) return trackLabel(progress, trackPhase.verb, trackPhase.uncounted)
  return PHASE_LABELS[progress.phase] ?? 'Syncing'
}

export function syncFraction(progress: SyncProgress | null): number | null {
  if (!progress?.total) return null
  const { done = 0, total, file } = progress
  const throughFile = file?.total ? Math.min(file.done / file.total, 1) : 0
  return Math.min((done + throughFile) / total, 1)
}
