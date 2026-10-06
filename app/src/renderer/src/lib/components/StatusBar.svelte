<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { formatSize, formatTotal } from '$lib/format'

  const { library, player } = getApp()

  const totals = $derived.by(() => {
    let duration = 0
    let size = 0
    for (const t of library.visible) {
      duration += t.duration
      size += t.fileSize
    }
    return { duration, size }
  })

  const message = $derived(player.editError ?? library.lockError ?? player.audioError)
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

  <span class="ml-auto truncate" title={library.info?.dbPath}>
    {library.info?.rekordboxRunning ? 'rekordbox is running' : 'rekordbox is closed'}
    {#if library.info}
      · {library.writable ? 'editing' : 'read-only'}
    {/if}
  </span>
</footer>
