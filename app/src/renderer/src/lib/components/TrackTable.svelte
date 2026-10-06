<script lang="ts">
  import { ArrowUp, ArrowDown, Star, AudioLines, Music } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import { formatBpm, formatDuration, keyColor, TRACK_COLORS } from '$lib/format'
  import type { SortKey } from '$lib/library.svelte'
  import type { Track } from '$lib/types'

  let { onselect, onactivate }: { onselect: (track: Track) => void; onactivate: (track: Track) => void } = $props()

  const { library, player } = getApp()

  type Column = {
    id: string
    label: string
    width: string
    align?: 'right' | 'center'
    sort?: SortKey
  }

  const columns: Column[] = [
    { id: 'index', label: '#', width: '44px', align: 'right' },
    { id: 'color', label: '', width: '10px', sort: 'color' },
    { id: 'art', label: '', width: '34px' },
    { id: 'title', label: 'Title', width: 'minmax(220px, 2.2fr)', sort: 'title' },
    { id: 'artist', label: 'Artist', width: 'minmax(150px, 1.4fr)', sort: 'artist' },
    { id: 'album', label: 'Album', width: 'minmax(120px, 1fr)', sort: 'album' },
    { id: 'genre', label: 'Genre', width: '120px', sort: 'genre' },
    { id: 'bpm', label: 'BPM', width: '64px', align: 'right', sort: 'bpm' },
    { id: 'key', label: 'Key', width: '52px', align: 'center', sort: 'key' },
    { id: 'duration', label: 'Time', width: '60px', align: 'right', sort: 'duration' },
    { id: 'rating', label: 'Rating', width: '78px', sort: 'rating' },
    { id: 'dateAdded', label: 'Added', width: '92px', sort: 'dateAdded' },
    { id: 'fileType', label: 'Type', width: '58px', sort: 'fileType' }
  ]
  const template = columns.map((c) => c.width).join(' ')

  const ROW = 32
  const OVERSCAN = 12

  let viewport = $state<HTMLDivElement>()
  let scrollTop = $state(0)
  let height = $state(0)

  const list = $derived(library.visible)
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN))
  const last = $derived(Math.min(list.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN))
  const rows = $derived(list.slice(first, last))

  export function ensureVisible(index: number): void {
    if (!viewport) return
    const header = ROW
    const top = index * ROW
    const visibleH = viewport.clientHeight - header
    if (top < viewport.scrollTop) viewport.scrollTop = top
    else if (top + ROW > viewport.scrollTop + visibleH) viewport.scrollTop = top + ROW - visibleH
  }

  export function scrollToTop(): void {
    if (viewport) viewport.scrollTop = 0
  }
</script>

{#snippet sortIcon(key: SortKey | undefined)}
  {#if key && library.sort?.key === key}
    {#if library.sort.dir === 1}
      <ArrowUp class="size-3 shrink-0 text-accent" />
    {:else}
      <ArrowDown class="size-3 shrink-0 text-accent" />
    {/if}
  {/if}
{/snippet}

{#snippet cell(column: Column, track: Track, index: number)}
  {#if column.id === 'index'}
    {#if player.track?.id === track.id}
      <AudioLines class={['ml-auto size-3.5 text-accent', player.playing && 'animate-pulse']} />
    {:else}
      <span class="text-faint">{index + 1}</span>
    {/if}
  {:else if column.id === 'color'}
    {#if TRACK_COLORS[track.color]}
      <span class="block h-4 w-[3px] rounded-full" style:background={TRACK_COLORS[track.color].hex}></span>
    {/if}
  {:else if column.id === 'art'}
    {#if track.hasArtwork}
      <img
        src={artworkUrl(track.id, 's')}
        alt=""
        loading="lazy"
        decoding="async"
        draggable="false"
        class="size-6 rounded-[3px] bg-raised object-cover"
      />
    {:else}
      <span class="grid size-6 place-items-center rounded-[3px] bg-raised text-faint">
        <Music class="size-3" />
      </span>
    {/if}
  {:else if column.id === 'title'}
    <span class="truncate font-medium text-fg">{track.title}</span>
    {#if track.remixer}
      <span class="ml-1.5 truncate text-faint">{track.remixer}</span>
    {/if}
  {:else if column.id === 'bpm'}
    <span class="text-fg/90">{formatBpm(track.bpm)}</span>
  {:else if column.id === 'key'}
    <span class="font-medium" style:color={keyColor(track.key) ?? 'inherit'}>{track.key}</span>
  {:else if column.id === 'duration'}
    {formatDuration(track.duration)}
  {:else if column.id === 'rating'}
    <span class="flex gap-px">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <Star
          class={['size-3', n <= track.rating ? 'fill-warn text-warn' : 'text-line']}
          strokeWidth={n <= track.rating ? 0 : 2}
        />
      {/each}
    </span>
  {:else if column.id === 'fileType'}
    <span class="rounded border border-line px-1 py-px text-[10px] font-medium text-faint">{track.fileType}</span>
  {:else}
    <span class="truncate">{track[column.id as keyof Track]}</span>
  {/if}
{/snippet}

<div
  bind:this={viewport}
  bind:clientHeight={height}
  class="tabular relative h-full overflow-auto"
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
>
  <div class="min-w-[1100px]">
    <div
      class="sticky top-0 z-10 grid h-8 items-center border-b border-line-soft bg-bg/95 text-[11px] font-medium text-faint backdrop-blur"
      style:grid-template-columns={template}
      role="row"
    >
      {#each columns as column (column.id)}
        {#if column.sort}
          <button
            class={[
              'flex h-full min-w-0 items-center gap-1 px-2 hover:text-fg',
              column.align === 'right' && 'flex-row-reverse',
              column.align === 'center' && 'justify-center',
              library.sort?.key === column.sort && 'text-fg'
            ]}
            onclick={() => library.toggleSort(column.sort!)}
          >
            <span class="truncate">{column.label}</span>
            {@render sortIcon(column.sort)}
          </button>
        {:else}
          <span class={['truncate px-2', column.align === 'right' && 'text-right']}>{column.label}</span>
        {/if}
      {/each}
    </div>

    <div class="relative" style:height="{list.length * ROW}px" role="rowgroup">
      <div class="absolute inset-x-0 top-0" style:transform="translateY({first * ROW}px)">
        {#each rows as track, i (track.id)}
          {@const index = first + i}
          {@const selected = library.selectedId === track.id}
          <div
            class={[
              'grid cursor-default items-center text-[12.5px] text-muted',
              selected ? 'bg-select text-fg' : 'hover:bg-surface'
            ]}
            style:grid-template-columns={template}
            style:height="{ROW}px"
            role="row"
            tabindex="-1"
            aria-selected={selected}
            onmousedown={() => onselect(track)}
            ondblclick={() => onactivate(track)}
          >
            {#each columns as column (column.id)}
              <div
                class={[
                  'flex min-w-0 items-center overflow-hidden px-2 whitespace-nowrap',
                  column.align === 'right' && 'justify-end',
                  column.align === 'center' && 'justify-center',
                  column.id === 'color' && 'px-0'
                ]}
              >
                {@render cell(column, track, index)}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    </div>

    {#if library.status === 'ready' && !library.sourceLoading && list.length === 0}
      <div class="absolute inset-x-0 top-24 text-center text-[13px] text-faint">
        {library.query ? `No tracks match “${library.query}”` : 'No tracks here'}
      </div>
    {/if}
  </div>
</div>
