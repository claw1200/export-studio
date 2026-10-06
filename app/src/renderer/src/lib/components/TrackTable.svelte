<script lang="ts">
  import { ArrowUp, ArrowDown, Check, Star, AudioLines, Music } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import { formatBpm, formatDuration, keyColor, TRACK_COLORS } from '$lib/format'
  import type { SortKey } from '$lib/library.svelte'
  import type { Track } from '$lib/types'

  let { onselect, onactivate }: { onselect: (track: Track) => void; onactivate: (track: Track) => void } = $props()

  const { library, player } = getApp()

  type Column = {
    id: string
    name: string
    label: string
    width: number
    min: number
    max: number
    fr?: number
    align?: 'right' | 'center'
    sort?: SortKey
  }

  const COLUMNS: Column[] = [
    { id: 'index', name: '#', label: '#', width: 44, min: 36, max: 72, align: 'right' },
    { id: 'color', name: 'Colour', label: '', width: 10, min: 10, max: 28, sort: 'color' },
    { id: 'art', name: 'Artwork', label: '', width: 34, min: 28, max: 64 },
    { id: 'title', name: 'Title', label: 'Title', width: 220, min: 120, max: 640, fr: 2.2, sort: 'title' },
    { id: 'artist', name: 'Artist', label: 'Artist', width: 150, min: 80, max: 480, fr: 1.4, sort: 'artist' },
    { id: 'album', name: 'Album', label: 'Album', width: 120, min: 72, max: 480, fr: 1, sort: 'album' },
    { id: 'genre', name: 'Genre', label: 'Genre', width: 120, min: 56, max: 320, sort: 'genre' },
    { id: 'bpm', name: 'BPM', label: 'BPM', width: 64, min: 48, max: 120, align: 'right', sort: 'bpm' },
    { id: 'key', name: 'Key', label: 'Key', width: 52, min: 40, max: 120, align: 'center', sort: 'key' },
    { id: 'duration', name: 'Time', label: 'Time', width: 60, min: 48, max: 120, align: 'right', sort: 'duration' },
    { id: 'rating', name: 'Rating', label: 'Rating', width: 78, min: 64, max: 140, sort: 'rating' },
    { id: 'dateAdded', name: 'Added', label: 'Added', width: 92, min: 64, max: 180, sort: 'dateAdded' },
    { id: 'fileType', name: 'Type', label: 'Type', width: 58, min: 44, max: 120, sort: 'fileType' }
  ]

  const STORAGE_KEY = 'browserColumns'

  function loadPrefs(): { widths: Record<string, number>; sized: string[]; hidden: string[] } {
    const widths = Object.fromEntries(COLUMNS.map((column) => [column.id, column.width]))
    let sized: string[] = []
    let hidden: string[] = []
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        const prefs = JSON.parse(raw) as {
          widths?: Record<string, unknown>
          sized?: unknown
          hidden?: unknown
        }
        if (prefs.widths && typeof prefs.widths === 'object') {
          for (const column of COLUMNS) {
            const width = prefs.widths[column.id]
            if (typeof width === 'number' && Number.isFinite(width)) {
              widths[column.id] = Math.min(column.max, Math.max(column.min, Math.round(width)))
            }
          }
        }
        if (Array.isArray(prefs.sized)) {
          sized = prefs.sized.filter(
            (id): id is string => typeof id === 'string' && COLUMNS.some((column) => column.id === id && column.fr)
          )
        }
        if (Array.isArray(prefs.hidden)) {
          hidden = prefs.hidden.filter((id): id is string => typeof id === 'string' && widths[id] !== undefined)
          if (hidden.length >= COLUMNS.length) hidden = []
        }
      }
    } catch {
      return { widths, sized, hidden }
    }
    return { widths, sized, hidden }
  }

  const initial = loadPrefs()
  let widths = $state(initial.widths)
  let sized = $state(initial.sized)
  let hidden = $state(initial.hidden)

  const shownColumns = $derived(COLUMNS.filter((column) => !hidden.includes(column.id)))
  const template = $derived(
    shownColumns
      .map((column) => {
        const width = widths[column.id]
        return column.fr && !sized.includes(column.id) ? `minmax(${column.width}px, ${column.fr}fr)` : `${width}px`
      })
      .join(' ')
  )
  const minWidth = $derived(
    shownColumns.reduce((sum, column) => {
      if (column.fr && !sized.includes(column.id)) return sum + column.width
      return sum + widths[column.id]
    }, 0)
  )

  function save(): void {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ widths, sized, hidden }))
  }

  function toggleColumn(id: string): void {
    if (hidden.includes(id)) hidden = hidden.filter((item) => item !== id)
    else if (hidden.length < COLUMNS.length - 1) hidden = [...hidden, id]
    save()
  }

  function resetColumns(): void {
    widths = Object.fromEntries(COLUMNS.map((column) => [column.id, column.width]))
    sized = []
    hidden = []
    save()
  }

  let drag = $state<{ id: string; startX: number; startW: number; wasSized: boolean } | null>(null)

  function startResize(e: PointerEvent, column: Column): void {
    if (e.button !== 0 || e.detail > 1) return
    e.preventDefault()
    e.stopPropagation()
    const handle = e.currentTarget as HTMLElement
    handle.setPointerCapture(e.pointerId)
    const cell = handle.parentElement
    const measured = cell?.getBoundingClientRect().width ?? widths[column.id]
    const startW = Math.min(column.max, Math.max(column.min, Math.round(measured)))
    const wasSized = sized.includes(column.id)
    widths[column.id] = startW
    if (column.fr && !wasSized) sized = [...sized, column.id]
    drag = { id: column.id, startX: e.clientX, startW, wasSized }
  }

  function onResizeMove(e: PointerEvent): void {
    const current = drag
    if (!current) return
    const column = COLUMNS.find((item) => item.id === current.id)
    if (!column) return
    const next = Math.round(current.startW + e.clientX - current.startX)
    widths[column.id] = Math.min(column.max, Math.max(column.min, next))
  }

  function endResize(): void {
    const current = drag
    if (!current) return
    drag = null
    const column = COLUMNS.find((item) => item.id === current.id)
    if (column?.fr && !current.wasSized && widths[current.id] === current.startW) {
      sized = sized.filter((id) => id !== current.id)
    }
    save()
  }

  function resetWidth(e: MouseEvent, column: Column): void {
    e.preventDefault()
    e.stopPropagation()
    widths[column.id] = column.width
    if (column.fr) sized = sized.filter((id) => id !== column.id)
    save()
  }

  let menu = $state<{ x: number; y: number } | null>(null)

  function openMenu(e: MouseEvent): void {
    e.preventDefault()
    if (drag) return
    const margin = 8
    const width = 192
    const height = Math.min(440, COLUMNS.length * 28 + 72)
    let x = e.clientX
    let y = e.clientY
    if (x + width > window.innerWidth - margin) x = window.innerWidth - width - margin
    if (y + height > window.innerHeight - margin) y = e.clientY - height
    menu = { x: Math.max(margin, x), y: Math.max(margin, y) }
  }

  function onMenuKey(e: KeyboardEvent): void {
    if (menu && e.key === 'Escape') {
      e.preventDefault()
      menu = null
    }
  }

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

<svelte:window onpointermove={onResizeMove} onpointerup={endResize} onpointercancel={endResize} onkeydown={onMenuKey} />

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

<div class="relative h-full">
  <div
    bind:this={viewport}
    bind:clientHeight={height}
    class="tabular relative h-full overflow-auto"
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  >
    <div style:min-width="max(100%, {minWidth}px)">
      <div
        class="sticky top-0 z-10 grid h-8 items-center border-b border-line-soft bg-bg/95 text-[11px] font-medium text-faint backdrop-blur"
        style:grid-template-columns={template}
        role="row"
        tabindex="-1"
        oncontextmenu={openMenu}
      >
        {#each shownColumns as column (column.id)}
          <div class="relative flex h-full min-w-0 items-center">
            {#if column.sort}
              <button
                class={[
                  'flex h-full min-w-0 flex-1 items-center gap-1 px-2 hover:text-fg',
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
              <span class={['min-w-0 flex-1 truncate px-2', column.align === 'right' && 'text-right']}>
                {column.label}
              </span>
            {/if}
            <div
              class="group/resize absolute top-0 -right-1 z-20 h-full w-2 cursor-col-resize touch-none"
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize {column.name}"
              title="Drag to resize. Double-click to reset."
              onpointerdown={(e) => startResize(e, column)}
              ondblclick={(e) => resetWidth(e, column)}
            >
              <span
                class={[
                  'pointer-events-none absolute inset-y-1 left-1/2 w-px -translate-x-1/2',
                  drag?.id === column.id ? 'bg-accent' : 'bg-line opacity-0 group-hover/resize:opacity-100'
                ]}
              ></span>
            </div>
          </div>
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
              onmousedown={(e) => {
                if (e.button === 0) onselect(track)
              }}
              ondblclick={() => onactivate(track)}
            >
              {#each shownColumns as column (column.id)}
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

  {#if menu}
    <button
      class="fixed inset-0 z-40 cursor-default"
      aria-label="Close columns menu"
      onclick={() => (menu = null)}
      oncontextmenu={(e) => {
        e.preventDefault()
        e.stopPropagation()
        menu = null
      }}
    ></button>
    <div
      class="fixed z-50 max-h-[min(440px,calc(100vh-16px))] w-48 overflow-y-auto rounded-lg border border-line bg-surface p-1"
      style:left="{menu.x}px"
      style:top="{menu.y}px"
      role="menu"
      tabindex="-1"
      aria-label="Columns"
      oncontextmenu={(e) => {
        e.preventDefault()
        e.stopPropagation()
      }}
    >
      <div class="px-2 pt-1 pb-1 text-[11px] font-medium text-faint">Columns</div>
      {#each COLUMNS as column (column.id)}
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-[12.5px] text-fg hover:bg-hover"
          role="menuitemcheckbox"
          aria-checked={!hidden.includes(column.id)}
          onclick={() => toggleColumn(column.id)}
        >
          <span class="grid size-3.5 shrink-0 place-items-center text-accent">
            {#if !hidden.includes(column.id)}
              <Check class="size-3.5" strokeWidth={2.5} />
            {/if}
          </span>
          <span class="truncate">{column.name}</span>
        </button>
      {/each}
      <div class="mx-2 my-1 border-t border-line-soft"></div>
      <button
        type="button"
        class="w-full rounded-md px-2 py-1 text-left text-[12.5px] text-muted hover:bg-hover hover:text-fg"
        role="menuitem"
        onclick={resetColumns}
      >
        Reset columns
      </button>
    </div>
  {/if}

  {#if drag}
    <div class="fixed inset-0 z-50 cursor-col-resize"></div>
  {/if}
</div>
