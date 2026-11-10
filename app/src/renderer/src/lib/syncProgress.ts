import type { SyncProgress } from './types'

const PHASE_LABELS: Record<string, string> = {
  format: 'Formatting the USB',
  reading: 'Reading the volume',
  writing: 'Writing the library',
  staged: 'Checking the export',
  verifying: 'Checking the export',
  publishing: 'Publishing',
  done: 'Finalizing sync',
  eject: 'Ejecting device'
}

function countedLabel({ done = 0, total = 0, name }: SyncProgress, verb: string, uncounted: string): string {
  if (!total) return uncounted
  return `${verb} ${Math.min(done + 1, total)} of ${total}${name ? ` · ${name}` : ''}`
}

export function syncPhaseLabel(progress: SyncProgress | null): string {
  if (!progress) return 'Preparing sync'
  switch (progress.phase) {
    case 'copy':
      return countedLabel(progress, 'Copying', 'Copying audio')
    case 'convert':
      return countedLabel(progress, 'Converting', 'Converting audio')
    case 'import':
      return countedLabel(progress, 'Importing', 'Importing')
  }
  return PHASE_LABELS[progress.phase] ?? 'Syncing'
}

export function syncFraction(progress: SyncProgress | null): number | null {
  if (!progress?.total) return null
  return (progress.done ?? 0) / progress.total
}
