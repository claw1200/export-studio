<script lang="ts">
  import type { ClassValue } from 'svelte/elements'
  import { Music } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { getPlayer } from '$lib/deck/context'
  import { formatClock } from '$lib/format'

  let { class: className, size = 'md' }: { class?: ClassValue; size?: 'md' | 'lg' } = $props()

  const player = getPlayer()
  const track = $derived(player.track)
  const lg = $derived(size === 'lg')
</script>

<div class={['flex min-w-0 items-center', lg ? 'gap-3' : 'gap-2.5', className]}>
  <div class={['shrink-0 overflow-hidden bg-raised', lg ? 'size-[68px] rounded-lg' : 'size-10 rounded-md']}>
    {#if track?.hasArtwork}
      <img src={artworkUrl(track.id, 'm')} alt="" class="size-full object-cover" draggable="false" />
    {:else}
      <div class="grid size-full place-items-center text-faint"><Music class={lg ? 'size-6' : 'size-4'} /></div>
    {/if}
  </div>
  <div class="min-w-0 flex-1">
    {#if track}
      <div class={['truncate font-semibold text-fg', lg ? 'text-[16px]' : 'text-[13.5px]']} title={track.title}>
        {track.title}
      </div>
      <div class={['tabular flex items-center gap-1.5', lg ? 'mt-1 text-[12.5px]' : 'mt-0.5 text-[11px]']}>
        <span class="truncate text-muted">{track.artist || 'Unknown artist'}</span>
        {#if track.fileType}
          <span class="shrink-0 rounded bg-raised px-1 text-[10px] font-semibold text-faint uppercase"
            >{track.fileType}</span
          >
        {/if}
      </div>
    {:else}
      <div class={['text-faint', lg ? 'text-[14px]' : 'text-[13px]']}>No track loaded</div>
    {/if}
  </div>
  <div class={['tabular shrink-0 text-right font-mono', lg ? 'w-[88px]' : 'w-[72px]']}>
    <div class={['font-medium text-fg', lg ? 'text-[18px]' : 'text-[14px]']}>{formatClock(player.position * 1000)}</div>
    <div class={['text-faint', lg ? 'mt-0.5 text-[12.5px]' : 'text-[11px]']}>
      −{formatClock(Math.max(0, player.duration - player.position) * 1000)}
    </div>
  </div>
</div>
