<script lang="ts">
  import { ArchiveRestore, Trash2 } from '@lucide/svelte'
  import { backupDate } from '$lib/backups.svelte'
  import { getApp } from '$lib/context'
  import { formatSize } from '$lib/format'
  import type { Backup, BackupReason } from '$lib/types'
  import ConfirmDialog from '../ConfirmDialog.svelte'

  const { library, backups } = getApp()

  const REASONS: Record<BackupReason, string> = {
    manual: 'Manual',
    'before-edit': 'Before editing',
    daily: 'Daily',
    'pre-restore': 'Replaced by a restore'
  }

  const rekordboxOpen = $derived(!!library.info?.rekordboxRunning)
  let confirming = $state.raw<{ kind: 'restore' | 'delete'; backup: Backup } | null>(null)

  function restoreMessage(backup: Backup): string {
    const other = backup.otherLibrary ? ` It was made from a different library (${backup.dbPath}).` : ''
    return `Replaces your rekordbox library with the one from ${backupDate(backup)}.${other} Your current library is kept in the backups folder.`
  }
</script>

{#each backups.list as backup (backup.id)}
  <div class="group/row flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover/60">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{backupDate(backup)}</span>
      <span class="mt-0.5 block truncate text-[11px] text-faint">
        {REASONS[backup.reason] ?? backup.reason}
        {#if backup.tracks != null}· {backup.tracks.toLocaleString()} tracks{/if}
        · {formatSize(backup.size)}
        {#if backup.otherLibrary}<span class="text-warn" title={backup.dbPath}> · other library</span>{/if}
        {#if !backup.verified}<span title="This library was not checked when it was saved"> · unchecked</span>{/if}
      </span>
    </span>
    <button
      class="flex h-6 items-center gap-1 rounded px-2 text-[11.5px] text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
      disabled={!!backups.busy || rekordboxOpen}
      title={rekordboxOpen ? 'Close rekordbox before restoring a backup' : 'Restore this backup'}
      onclick={() => (confirming = { kind: 'restore', backup })}
    >
      <ArchiveRestore class="size-3" />
      Restore
    </button>
    <button
      class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
      disabled={!!backups.busy}
      aria-label="Delete the backup from {backupDate(backup)}"
      title="Delete"
      onclick={() => (confirming = { kind: 'delete', backup })}
    >
      <Trash2 class="size-3" />
    </button>
  </div>
{:else}
  <p class="px-2 text-[12px] text-faint">No backups yet.</p>
{/each}

{#if confirming}
  {@const { kind, backup } = confirming}
  {#if kind === 'restore'}
    <ConfirmDialog
      title="Restore this backup?"
      message={restoreMessage(backup)}
      confirmLabel="Restore"
      danger
      onconfirm={() => void backups.restore(backup)}
      onclose={() => (confirming = null)}
    />
  {:else}
    <ConfirmDialog
      title="Delete this backup?"
      message="The backup from {backupDate(backup)} will be deleted. This can’t be undone."
      confirmLabel="Delete"
      danger
      onconfirm={() => void backups.remove(backup)}
      onclose={() => (confirming = null)}
    />
  {/if}
{/if}
