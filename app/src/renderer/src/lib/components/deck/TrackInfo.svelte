<script lang="ts">
  import type { ClassValue } from 'svelte/elements'
  import { Music } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { getPlayer } from '$lib/deck/context'
  import { formatClock } from '$lib/format'

  let { class: className }: { class?: ClassValue } = $props()

  const player = getPlayer()
  const track = $derived(player.track)
</script>

<div class={['flex min-w-0 items-center gap-2.5', className]}>
  <div class="size-10 shrink-0 overflow-hidden rounded-md bg-raised">
    {#if track?.hasArtwork}
      <img src={artworkUrl(track.id, 'm')} alt="" class="size-full object-cover" draggable="false" />
    {:else}
      <div class="grid size-full place-items-center text-faint"><Music class="size-4" /></div>
    {/if}
  </div>
  <div class="min-w-0 flex-1">
    {#if track}
      <div class="truncate text-[13.5px] font-semibold text-fg" title={track.title}>{track.title}</div>
      <div class="tabular mt-0.5 flex items-center gap-1.5 text-[11px]">
        <span class="truncate text-muted">{track.artist || 'Unknown artist'}</span>
        {#if track.fileType}
          <span class="shrink-0 rounded bg-raised px-1 text-[10px] font-semibold text-faint uppercase"
            >{track.fileType}</span
          >
        {/if}
      </div>
    {:else}
      <div class="text-[13px] text-faint">No track loaded</div>
    {/if}
  </div>
  <div class="tabular w-[72px] shrink-0 text-right font-mono">
    <div class="text-[14px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
    <div class="text-[11px] text-faint">−{formatClock(Math.max(0, player.duration - player.position) * 1000)}</div>
  </div>
</div>
