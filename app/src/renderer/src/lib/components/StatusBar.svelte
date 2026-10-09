<script lang="ts">
  import { AlertTriangle, LoaderCircle, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { formatSize, formatTotal } from '$lib/format'

  const { library, player, analysis, backups } = getApp()

  const totals = $derived.by(() => {
    let duration = 0
    let size = 0
    for (const t of library.visible) {
      duration += t.duration
      size += t.fileSize
    }
    return { duration, size }
  })

  const message = $derived(player.editError ?? library.editError ?? library.lockError ?? player.audioError)
</script>

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

  {#if message}
    <span
      class="absolute top-1/2 left-1/2 flex h-5 max-w-[40%] -translate-x-1/2 -translate-y-1/2 items-center gap-1.5 rounded-md border border-warn/60 bg-warn/10 px-2 text-warn"
      role="status"
      title={message}
    >
      <AlertTriangle class="size-3 shrink-0" />
      <span class="truncate">{message}</span>
    </span>
  {/if}

  {#if backups.progress}
    {@const { reason, done = 0, total = 0 } = backups.progress}
    <span class="ml-auto flex items-center gap-2 text-muted" role="status">
      <LoaderCircle class="size-3 animate-spin text-accent" />
      <span>{reason === 'restore' ? 'Restoring backup' : 'Backing up library'}</span>
      {#if total}
        <span class="h-1 w-20 overflow-hidden rounded-full bg-line">
          <span class="block h-full rounded-full bg-accent transition-[width]" style:width="{(100 * done) / total}%"
          ></span>
        </span>
      {/if}
    </span>
  {/if}

  {#if analysis.running && analysis.progress}
    {@const { total, done, failed } = analysis.progress}
    <span class="ml-auto flex items-center gap-2 text-muted" role="status">
      <LoaderCircle class="size-3 animate-spin text-accent" />
      <span>Analysing {Math.min(done + failed + 1, total).toLocaleString()} of {total.toLocaleString()}</span>
      <span class="h-1 w-20 overflow-hidden rounded-full bg-line">
        <span
          class="block h-full rounded-full bg-accent transition-[width]"
          style:width="{(100 * (done + failed)) / Math.max(total, 1)}%"
        ></span>
      </span>
      {#if failed}
        <span class="text-warn" title={analysis.lastError ?? undefined}>{failed} failed</span>
      {/if}
      <button
        type="button"
        class="grid size-4 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
        title="Stop analysing"
        aria-label="Stop analysing"
        onclick={() => analysis.cancel()}
      >
        <X class="size-3" />
      </button>
    </span>
  {/if}

  <span class="truncate {analysis.running || backups.progress ? '' : 'ml-auto'}" title={library.info?.dbPath}>
    {library.info?.rekordboxRunning ? 'rekordbox is running' : 'rekordbox is closed'}
    {#if library.info}
      · {library.writable ? 'editing' : 'read-only'}
    {/if}
  </span>
</footer>
