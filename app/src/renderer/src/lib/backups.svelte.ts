import { api } from './api'
import type { Analysis } from './analysis.svelte'
import type { Library } from './library.svelte'
import type { Player } from './player.svelte'
import type { Backup, BackupProgress, BackupSettings } from './types'

export const KEEP_CHOICES = [5, 10, 20] as const
export const INTERVAL_CHOICES = [
  { hours: 1, label: '1h' },
  { hours: 3, label: '3h' },
  { hours: 6, label: '6h' },
  { hours: 12, label: '12h' },
  { hours: 24, label: '1d' },
  { hours: 48, label: '2d' }
] as const

export function backupDate(backup: Backup): string {
  return new Date(backup.created * 1000).toLocaleString([], {
    dateStyle: 'medium',
    timeStyle: 'short'
  })
}

/** Backups of the rekordbox library, made and restored by the engine. */
export class Backups {
  list = $state.raw<Backup[]>([])
  settings = $state<BackupSettings>({ auto: true, keep: 10, interval: 12 })
  progress = $state.raw<BackupProgress | null>(null)
  busy = $state<'backup' | 'restore' | null>(null)
  error = $state<string | null>(null)

  constructor(
    private library: Library,
    private player: Player,
    private analysis: Analysis
  ) {
    api.onEvent((event, data) => {
      if (event === 'backup.progress') {
        const progress = data as BackupProgress
        this.progress = progress.stage === 'done' ? null : progress
      }
      if (event === 'backup.changed') void this.refresh()
    })
  }

  async refresh(): Promise<void> {
    try {
      const [list, settings] = await Promise.all([api.backupList(), api.backupSettings()])
      this.list = list
      this.settings = settings
    } catch (e) {
      this.error = (e as Error).message
    }
  }

  async setSettings(change: Partial<BackupSettings>): Promise<void> {
    try {
      this.settings = await api.setBackupSettings(change)
      await this.refresh()
    } catch (e) {
      this.error = (e as Error).message
    }
  }

  async create(): Promise<void> {
    if (this.busy) return
    this.busy = 'backup'
    this.error = null
    try {
      await api.backupCreate()
      await this.refresh()
    } catch (e) {
      this.error = (e as Error).message
    } finally {
      this.busy = null
    }
  }

  async remove(backup: Backup): Promise<void> {
    try {
      this.list = await api.backupDelete(backup.id)
    } catch (e) {
      this.error = (e as Error).message
    }
  }

  async restore(backup: Backup): Promise<boolean> {
    if (this.busy) return false
    this.busy = 'restore'
    this.error = null
    this.analysis.cancel()
    this.player.eject()
    try {
      const result = await api.backupRestore(backup.id)
      this.library.notify(`Restored the library from ${backupDate(backup)}: ${result.tracks.toLocaleString()} tracks`)
      return true
    } catch (e) {
      this.error = (e as Error).message
      return false
    } finally {
      this.busy = null
      await this.library.load(true)
      await this.refresh()
    }
  }
}
