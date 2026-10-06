<script lang="ts">
  import { getApp } from '$lib/context'
  import { formatSize, formatTotal } from '$lib/format'

  const { library } = getApp()

  const totals = $derived.by(() => {
    let duration = 0
    let size = 0
    for (const t of library.visible) {
      duration += t.duration
      size += t.fileSize
    }
    return { duration, size }
  })
</script>

<footer
  class="tabular flex h-7 shrink-0 items-center gap-4 border-t border-line-soft bg-bg px-3 text-[11px] text-faint"
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
  {#if library.lockError}
    <span class="ml-auto truncate text-warn" title={library.lockError}>{library.lockError}</span>
  {:else}
    <span class="ml-auto truncate" title={library.info?.dbPath}>
      {library.info?.rekordboxRunning ? 'rekordbox is running' : 'rekordbox is closed'}
      {#if library.info}
        · {library.writable ? 'write' : 'read-only'}
      {/if}
    </span>
  {/if}
</footer>
