<script lang="ts">
  import { AlertTriangle, CircleAlert, CircleArrowUp, Info, LoaderCircle, X } from '@lucide/svelte'
  import type { NoticeLevel } from '$lib/library.svelte'
  import { getApp } from '$lib/context'
  import { formatSize, formatTotal } from '$lib/format'
  import { syncFraction, syncPhaseLabel } from '$lib/syncProgress'
  import type { Snippet } from 'svelte'
  import type { Update } from '../../../../shared/updates'

  const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000

  const { library, decks, analysis, backups } = getApp()
  const player = $derived(decks.focused)

  let update = $state<Update | null>(null)

  $effect(() => {
    const check = (): void => void window.api.checkForUpdate().then((found) => (update = found))
    check()
    const timer = setInterval(check, UPDATE_CHECK_INTERVAL_MS)
    return () => clearInterval(timer)
  })

  const showingProgress = $derived(
    analysis.running || !!backups.progress || !!library.convertProgress || library.syncing
  )
  const syncDevice = $derived(library.devices.find((device) => device.id === library.syncVolumeId))

  const totals = $derived.by(() => {
    let duration = 0
    let size = 0
    for (const t of library.visible) {
      duration += t.duration
      size += t.fileSize
    }
    return { duration, size }
  })

  const notice = $derived.by((): { message: string; level: NoticeLevel } | null => {
    if (player?.editError) return { message: player.editError, level: 'error' }
    if (library.editError) return { message: library.editError, level: library.editLevel }
    if (library.lockError) return { message: library.lockError, level: library.lockLevel }
    if (player?.audioError) return { message: player.audioError, level: 'error' }
    return null
  })

  const NOTICE_STYLES: Record<NoticeLevel, string> = {
    info: 'border-accent/60 bg-accent/10 text-accent',
    warning: 'border-warn/60 bg-warn/10 text-warn',
    error: 'border-danger/60 bg-danger/10 text-danger'
  }
</script>

{#snippet task(label: string, fraction: number | null, controls?: Snippet)}
  <span class="ml-auto flex min-w-0 items-center gap-2 text-muted" role="status">
    <LoaderCircle class="size-3 shrink-0 animate-spin text-accent" />
    <span class="truncate" title={label}>{label}</span>
    {#if fraction !== null}
      <span class="h-1 w-20 shrink-0 overflow-hidden rounded-full bg-line">
        <span class="block h-full rounded-full bg-accent transition-[width]" style:width="{100 * fraction}%"></span>
      </span>
    {/if}
    {@render controls?.()}
  </span>
{/snippet}

{#snippet analysisControls()}
  {#if analysis.progress?.failed}
    <span class="shrink-0 text-warn" title={analysis.lastError ?? undefined}>{analysis.progress.failed} failed</span>
  {/if}
  <button
    type="button"
    class="grid size-4 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
    title="Stop analysing"
    aria-label="Stop analysing"
    onclick={() => analysis.cancel()}
  >
    <X class="size-3" />
  </button>
{/snippet}

<footer
  class="tabular relative flex h-7 shrink-0 items-center gap-4 border-t border-line-soft bg-bg px-3 text-[11px] text-faint"
>
  <span class="font-medium text-muted">{library.sourceTitle}</span>
  <span>
    {library.visible.length.toLocaleString()}
    {library.query ? `of ${library.sourceTracks.length.toLocaleString()} ` : ''}tracks
  </span>
  <span>{formatTotal(totals.duration)}</span>
  <span>{formatSize(totals.size)}</span>
  {#if library.sourceLoading}
    <span class="text-accent">Loading…</span>
  {/if}

  {#if notice}
    <span
      class="absolute top-1/2 left-1/2 flex h-5 max-w-[40%] -translate-x-1/2 -translate-y-1/2 items-center gap-1.5 rounded-md border px-2 {NOTICE_STYLES[
        notice.level
      ]}"
      role={notice.level === 'error' ? 'alert' : 'status'}
      title={notice.message}
    >
      {#if notice.level === 'error'}
        <CircleAlert class="size-3 shrink-0" />
      {:else if notice.level === 'warning'}
        <AlertTriangle class="size-3 shrink-0" />
      {:else}
        <Info class="size-3 shrink-0" />
      {/if}
      <span class="truncate">{notice.message}</span>
    </span>
  {/if}

  {#if library.syncing}
    {@const label = syncPhaseLabel(library.syncProgress)}
    {@render task(syncDevice ? `${syncDevice.name} · ${label}` : label, syncFraction(library.syncProgress))}
  {/if}

  {#if backups.progress}
    {@const { reason, done = 0, total = 0 } = backups.progress}
    {@render task(reason === 'restore' ? 'Restoring backup' : 'Backing up library', total ? done / total : null)}
  {/if}

  {#if library.convertProgress}
    {@const { done, total } = library.convertProgress}
    {@render task(
      `Converting ${Math.min(done + 1, total).toLocaleString()} of ${total.toLocaleString()}`,
      done / Math.max(total, 1)
    )}
  {/if}

  {#if analysis.running && analysis.progress}
    {@const { total, done, failed } = analysis.progress}
    {@render task(
      `Analysing ${Math.min(done + failed + 1, total).toLocaleString()} of ${total.toLocaleString()}`,
      (done + failed) / Math.max(total, 1),
      analysisControls
    )}
  {/if}

  {#if update}
    <a
      class={['flex items-center gap-1 text-accent hover:underline', !showingProgress && 'ml-auto']}
      href={update.url}
      target="_blank"
      rel="noreferrer"
      title="Open the release page to download version {update.version}"
    >
      <CircleArrowUp class="size-3" />
      Update to {update.version}
    </a>
  {/if}

  <span class={['truncate', !showingProgress && !update && 'ml-auto']} title={library.info?.dbPath}>
    {library.info?.rekordboxRunning ? 'rekordbox is running' : 'rekordbox is closed'}
    {#if library.info}
      · {library.writable ? 'editing' : 'read-only'}
    {/if}
  </span>
</footer>
