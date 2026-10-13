<script lang="ts">
  import { flushSync } from 'svelte'
  import { ArrowUp, ArrowDown, Star, AudioLines, Music, X } from '@lucide/svelte'
  import { browserPreviews } from '$lib/browserPreview.svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import { assess, factsFromProbe, factsFromTrack, type Verdict } from '$lib/hardware/compatibility'
  import { formatBpm, formatDuration, keyColor, TRACK_COLORS } from '$lib/format'
  import { chordLabel } from '$lib/keymap'
  import { TRACK_DRAG_TYPE } from '$lib/library.svelte'
  import { droppedPaths, isFileDrag } from '$lib/library/drop'
  import type { SortKey } from '$lib/library/sort'
  import { menuAt, tidy, type MenuEntry, type MenuRequest } from '$lib/menu'
  import { getSetting, setSetting } from '$lib/settings'
  import { fileMenu, trackMenu, type TrackMenuShortcut } from '$lib/trackMenu'
  import type { Track } from '$lib/types'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import ContextMenu from './ContextMenu.svelte'
  import WaveformThumb from './WaveformThumb.svelte'

  let {
    onselect,
    onactivate,
    onedit,
    onautorelocate,
    onremove,
    onanalyse
  }: {
    onselect: (track: Track) => void
    onactivate: (track: Track) => void
    onedit: (tracks: Track[]) => void
    onautorelocate: (tracks: Track[]) => void
    onremove: (tracks: Track[]) => void
    onanalyse: (tracks: Track[]) => void
  } = $props()

  const { library, player, keybinds, analysis } = getApp()
  const isMac = window.api.platform === 'darwin'

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
    { id: 'index', name: '#', label: '#', width: 44, min: 36, max: 72, align: 'right', sort: 'position' },
    { id: 'color', name: 'Colour', label: 'Colour', width: 80, min: 72, max: 160, align: 'center', sort: 'color' },
    { id: 'art', name: 'Artwork', label: 'Artwork', width: 88, min: 76, max: 180 },
    { id: 'title', name: 'Title', label: 'Title', width: 220, min: 120, max: 640, fr: 2.2, sort: 'title' },
    { id: 'artist', name: 'Artist', label: 'Artist', width: 150, min: 80, max: 480, fr: 1.4, sort: 'artist' },
    { id: 'album', name: 'Album', label: 'Album', width: 120, min: 72, max: 480, fr: 1, sort: 'album' },
    { id: 'genre', name: 'Genre', label: 'Genre', width: 120, min: 56, max: 320, sort: 'genre' },
    { id: 'bpm', name: 'BPM', label: 'BPM', width: 64, min: 48, max: 120, align: 'right', sort: 'bpm' },
    { id: 'key', name: 'Key', label: 'Key', width: 52, min: 40, max: 120, align: 'center', sort: 'key' },
    { id: 'duration', name: 'Time', label: 'Time', width: 60, min: 48, max: 120, align: 'right', sort: 'duration' },
    { id: 'compat', name: 'Hardware', label: 'Hardware', width: 104, min: 96, max: 180, align: 'center', sort: 'compat' },
    { id: 'wave', name: 'Waveform', label: 'Waveform', width: 148, min: 80, max: 480, sort: 'cueCount' },
    { id: 'cue', name: 'CUE', label: 'CUE', width: 72, min: 52, max: 110, align: 'center', sort: 'cueStars' },
    { id: 'rating', name: 'Rating', label: 'Rating', width: 78, min: 64, max: 140, sort: 'rating' },
    { id: 'dateAdded', name: 'Added', label: 'Added', width: 92, min: 64, max: 180, sort: 'dateAdded' },
    { id: 'fileType', name: 'Type', label: 'Type', width: 58, min: 44, max: 120, sort: 'fileType' }
  ]

  const STORAGE_KEY = 'browserColumns'
  const LAYOUT_VERSION = 3
  const SHOWN_BY_DEFAULT = [
    'index',
    'compat',
    'cue',
    'wave',
    'art',
    'title',
    'artist',
    'bpm',
    'key',
    'duration',
    'dateAdded'
  ]

  function defaultOrder(): string[] {
    const order = SHOWN_BY_DEFAULT.filter((id) => COLUMNS.some((column) => column.id === id))
    for (const column of COLUMNS) if (!order.includes(column.id)) order.push(column.id)
    return order
  }

  function placeAfter(ids: string[], id: string, after: string): string[] {
    const next = ids.filter((item) => item !== id)
    const at = next.indexOf(after)
    next.splice(at + 1, 0, id)
    return next
  }

  function defaultHidden(): string[] {
    return COLUMNS.map((column) => column.id).filter((id) => !SHOWN_BY_DEFAULT.includes(id))
  }

  function loadPrefs(): { widths: Record<string, number>; sized: string[]; hidden: string[]; order: string[] } {
    const widths = Object.fromEntries(COLUMNS.map((column) => [column.id, column.width]))
    let sized: string[] = []
    let hidden = defaultHidden()
    let order = defaultOrder()
    try {
      const raw = getSetting(STORAGE_KEY)
      if (raw) {
        const prefs = JSON.parse(raw) as {
          layout?: unknown
          widths?: Record<string, unknown>
          sized?: unknown
          hidden?: unknown
          order?: unknown
        }
        const savedLayout = prefs.layout === 2 || prefs.layout === LAYOUT_VERSION
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
        if (savedLayout && Array.isArray(prefs.hidden)) {
          hidden = prefs.hidden.filter((id): id is string => typeof id === 'string' && widths[id] !== undefined)
          if (hidden.length >= COLUMNS.length) hidden = []
        }
        if (savedLayout && Array.isArray(prefs.order)) {
          const seen: Record<string, true> = {}
          const next: string[] = []
          for (const id of prefs.order) {
            if (typeof id === 'string' && widths[id] !== undefined && !seen[id]) {
              next.push(id)
              seen[id] = true
            }
          }
          for (const id of order) if (!seen[id]) next.push(id)
          order = prefs.layout === LAYOUT_VERSION ? next : placeAfter(next, 'compat', 'index')
        }
      }
    } catch {
      return { widths, sized, hidden, order }
    }
    return { widths, sized, hidden, order }
  }

  const initial = loadPrefs()
  let widths = $state(initial.widths)
  let sized = $state(initial.sized)
  let hidden = $state(initial.hidden)
  let order = $state(initial.order)

  const duplicateView = $derived(library.sourceView?.type === 'duplicates')

  function columnWidth(column: Column): number {
    const width = widths[column.id] ?? column.width
    return duplicateView && column.id === 'index' ? Math.max(width, 72) : width
  }

  function headerLabel(column: Column): string {
    return duplicateView && column.id === 'index' ? 'Group' : column.label
  }

  const orderedColumns = $derived.by(() => {
    const rank = new Map(order.map((id, index) => [id, index]))
    return [...COLUMNS].sort((a, b) => (rank.get(a.id) ?? 0) - (rank.get(b.id) ?? 0))
  })
  const shownColumns = $derived(orderedColumns.filter((column) => !hidden.includes(column.id)))
  const tableColumns = $derived(shownColumns)
  const template = $derived(
    tableColumns
      .map((column) => {
        const width = columnWidth(column)
        return column.fr && !sized.includes(column.id) ? `minmax(${column.width}px, ${column.fr}fr)` : `${width}px`
      })
      .join(' ')
  )
  const minWidth = $derived(
    tableColumns.reduce((sum, column) => {
      if (column.fr && !sized.includes(column.id)) return sum + column.width
      return sum + columnWidth(column)
    }, 0)
  )

  function save(): void {
    setSetting(STORAGE_KEY, JSON.stringify({ layout: LAYOUT_VERSION, widths, sized, hidden, order }))
  }

  function toggleColumn(id: string): void {
    if (hidden.includes(id)) hidden = hidden.filter((item) => item !== id)
    else if (hidden.length < COLUMNS.length - 1) hidden = [...hidden, id]
    save()
  }

  function resetColumns(): void {
    widths = Object.fromEntries(COLUMNS.map((column) => [column.id, column.width]))
    sized = []
    hidden = defaultHidden()
    order = defaultOrder()
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

  function columnItems(): MenuEntry[] {
    return [
      { kind: 'header', label: 'Columns' },
      ...orderedColumns.map((column) => ({
        label: column.name,
        checked: !hidden.includes(column.id),
        keepOpen: true,
        onselect: () => toggleColumn(column.id)
      })),
      { kind: 'separator' },
      { label: 'Reset columns', keepOpen: true, onselect: resetColumns }
    ]
  }

  function openColumnMenu(e: MouseEvent): void {
    e.preventDefault()
    if (!drag && !moving?.active) rowMenu = menuAt(e, 'Columns', columnItems)
  }

  let headerEl = $state<HTMLDivElement>()
  let moving = $state<{
    id: string
    startX: number
    startY: number
    active: boolean
    index: number
    dropX: number
  } | null>(null)
  let suppressSort = false

  function startMove(e: PointerEvent): void {
    if (e.button !== 0 || drag) return
    const target = e.target
    if (!(target instanceof Element) || target.closest('[role="separator"]')) return
    const id = target.closest<HTMLElement>('[data-column]')?.dataset.column
    if (!id) return
    moving = { id, startX: e.clientX, startY: e.clientY, active: false, index: 0, dropX: 0 }
  }

  function onPointerMove(e: PointerEvent): void {
    onResizeMove(e)
    const current = moving
    if (!current || !headerEl) return
    if (!current.active && Math.hypot(e.clientX - current.startX, e.clientY - current.startY) < 4) return
    const cells = [...headerEl.querySelectorAll<HTMLElement>('[data-column]')]
    const header = headerEl.getBoundingClientRect()
    let index = cells.length
    for (let i = 0; i < cells.length; i++) {
      const rect = cells[i].getBoundingClientRect()
      if (e.clientX < rect.left + rect.width / 2) {
        index = i
        break
      }
    }
    const edge = cells[Math.min(index, Math.max(0, cells.length - 1))]
    const rect = edge?.getBoundingClientRect()
    const dropX = rect ? (index >= cells.length ? rect.right : rect.left) - header.left : 0
    moving = { ...current, active: true, index, dropX }
  }

  function endPointer(): void {
    endResize()
    const current = moving
    moving = null
    if (!current?.active) return
    suppressSort = true
    setTimeout(() => {
      suppressSort = false
    }, 0)
    const shown = shownColumns.map((column) => column.id)
    const from = shown.indexOf(current.id)
    const to = current.index
    if (from < 0 || to === from || to === from + 1) return
    const before = to >= shown.length ? null : shown[to]
    const next = order.filter((id) => id !== current.id)
    const at = before ? next.indexOf(before) : next.length
    next.splice(at < 0 ? next.length : at, 0, current.id)
    order = next
    save()
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
  const wantsPreview = $derived(shownColumns.some((column) => column.id === 'wave' || column.id === 'cue'))
  const wantsCompat = $derived(shownColumns.some((column) => column.id === 'compat'))
  let compatTip = $state<{ id: string; top: number; left: number; above: boolean } | null>(null)
  const shownTip = $derived.by(() => {
    const tip = compatTip
    if (!tip || !rows.some((track) => track.id === tip.id)) return null
    return tip
  })

  function compatibilityOf(track: Track): Verdict {
    const probe = library.probeOf(track)
    const facts = probe?.audio ? factsFromProbe(probe.audio, track) : factsFromTrack(track)
    return assess(facts, {
      missing: track.missing || !!probe?.missing,
      unreadable: !!probe?.unreadable,
      streaming: track.streaming,
      greenId: library.hardwareGreen
    })
  }

  function showCompat(node: HTMLElement, id: string): void {
    const rect = node.getBoundingClientRect()
    const above = rect.bottom + 150 > window.innerHeight
    compatTip = {
      id,
      top: above ? rect.top - 6 : rect.bottom + 6,
      left: Math.max(8, Math.min(rect.left - 8, window.innerWidth - 280)),
      above
    }
  }

  function hideCompat(id: string): void {
    if (compatTip?.id === id) compatTip = null
  }

  $effect(() => {
    if (!wantsPreview) return
    const ids = rows.filter((track) => track.collected !== false).map((track) => track.id)
    void browserPreviews.revision
    browserPreviews.request(ids)
  })

  $effect(() => {
    if (!wantsCompat) return
    library.probeVisible(rows.filter((track) => track.collected !== false).map((track) => track.id))
  })

  const CUE_STARS = [
    { key: 'memory' as const, color: '#ff3b3b', label: 'Memory cue' },
    { key: 'hot' as const, color: '#2ee56b', label: 'Hot cue' },
    { key: 'phrase' as const, color: '#c084fc', label: 'Phrase analysis' }
  ]

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

  let pendingCollapse: string | null = null

  function rowMousedown(e: MouseEvent, track: Track): void {
    if (e.button !== 0) return
    pendingCollapse = null
    if (e.shiftKey) library.selectRange(track.id)
    else if (isMac ? e.metaKey : e.ctrlKey) library.toggleSelected(track.id)
    else if (library.selection.has(track.id) && library.selection.size > 1) pendingCollapse = track.id
    else onselect(track)
  }

  function rowMouseup(e: MouseEvent, track: Track): void {
    if (e.button === 0 && pendingCollapse === track.id) onselect(track)
    pendingCollapse = null
  }

  let rowMenu = $state<MenuRequest | null>(null)
  let deletingFiles = $state<Track[] | null>(null)
  const trashName = window.api.platform === 'win32' ? 'Recycle Bin' : 'Trash'

  async function trashFiles(tracks: Track[]): Promise<void> {
    const paths = tracks.map((track) => track.path).filter(Boolean)
    const failed = await window.api.trash(paths)
    const gone = new Set(failed)
    library.dropFiles(paths.filter((path) => !gone.has(path)))
    if (failed.length) {
      const files = failed.length === 1 ? 'one file' : `${failed.length} files`
      library.notify(`Couldn’t move ${files} to the ${trashName}`)
    }
  }

  function shortcut(id: TrackMenuShortcut): string | undefined {
    const chord = keybinds.bindings[id][0]
    return chord ? chordLabel(chord) : undefined
  }

  function rowContextMenu(e: MouseEvent, track: Track): void {
    e.preventDefault()
    pendingCollapse = null
    if (!library.selection.has(track.id)) onselect(track)
    const selected = library.selectedTracks
    const loose = selected.filter((item) => item.collected === false)
    const owned = selected.filter((item) => item.collected !== false)
    rowMenu = menuAt(
      e,
      loose.length && !owned.length ? 'File' : 'Track',
      tidy([
        ...(owned.length
          ? trackMenu(library, player, owned, {
              edit: onedit,
              autoRelocate: onautorelocate,
              remove: onremove,
              analyse: onanalyse,
              shortcut
            })
          : []),
        owned.length && loose.length && { kind: 'separator' },
        ...(loose.length
          ? fileMenu(
              library,
              player,
              loose,
              {
                importFiles: (paths, playlistId) => void library.importFiles(paths, playlistId),
                importAndAnalyse: (paths) => {
                  void library.importFiles(paths).then((result) => {
                    if (result?.tracks.length) analysis.analyse(result.tracks)
                  })
                },
                deleteFiles: (files) => (deletingFiles = files)
              },
              owned.length === 0
            )
          : [])
      ])
    )
  }

  let ghost = $state<HTMLDivElement>()
  let ghostLabel = $state('')
  let dropIndex = $state<number | null>(null)
  const dragged = $derived(new Set(library.dragging ?? []))
  const AUTOSCROLL_EDGE = 40
  const AUTOSCROLL_STEP = 14

  function dragStart(e: DragEvent, track: Track): void {
    if (library.source.type === 'device' || track.collected === false) {
      e.preventDefault()
      return
    }
    pendingCollapse = null
    if (!library.selection.has(track.id)) onselect(track)
    const ids = library.selectedTracks.filter((item) => item.collected !== false).map((item) => item.id)
    if (!ids.length) {
      e.preventDefault()
      return
    }
    library.dragging = ids
    const transfer = e.dataTransfer
    if (!transfer) return
    transfer.effectAllowed = 'copyMove'
    transfer.setData(TRACK_DRAG_TYPE, JSON.stringify(ids))
    ghostLabel = ids.length === 1 ? track.title : `${ids.length} tracks`
    flushSync()
    if (ghost) transfer.setDragImage(ghost, -12, -8)
  }

  function dragEnd(): void {
    library.dragging = null
    dropIndex = null
  }

  function rowsDragOver(e: DragEvent): void {
    if (!library.dragging || !library.canReorder) return
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
    const top = (e.currentTarget as HTMLElement).getBoundingClientRect().top
    dropIndex = Math.max(0, Math.min(list.length, Math.round((e.clientY - top) / ROW)))
  }

  function rowsDragLeave(e: DragEvent): void {
    const into = e.relatedTarget
    if (!(into instanceof Node) || !(e.currentTarget as HTMLElement).contains(into)) dropIndex = null
  }

  function rowsDrop(e: DragEvent): void {
    const ids = library.dragging
    const playlist = library.sourcePlaylist
    const index = dropIndex
    dropIndex = null
    if (!ids || !playlist || index === null || !library.canReorder) return
    e.preventDefault()
    void library.moveInPlaylist(playlist.id, ids, list[index]?.id ?? null)
  }

  let rowsEl = $state<HTMLDivElement>()
  let fileDrop = $state(false)
  const fileTarget = $derived(
    library.source.type === 'collection'
      ? 'collection'
      : library.sourcePlaylist?.kind === 'playlist'
        ? 'playlist'
        : null
  )

  function fileDragOver(e: DragEvent): void {
    if (library.dragging || !fileTarget || !isFileDrag(e)) return
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
    fileDrop = true
    if (fileTarget === 'playlist' && library.canReorder && rowsEl) {
      const top = rowsEl.getBoundingClientRect().top
      dropIndex = Math.max(0, Math.min(list.length, Math.round((e.clientY - top) / ROW)))
    }
  }

  function fileDragLeave(e: DragEvent): void {
    const into = e.relatedTarget
    if (into instanceof Node && (e.currentTarget as HTMLElement).contains(into)) return
    fileDrop = false
    if (!library.dragging) dropIndex = null
  }

  function fileDropped(e: DragEvent): void {
    if (!fileDrop) return
    e.preventDefault()
    fileDrop = false
    const index = dropIndex
    dropIndex = null
    const paths = droppedPaths(e)
    const playlist = library.sourcePlaylist
    if (fileTarget === 'playlist' && playlist) {
      const before = index === null ? undefined : (list[index]?.id ?? null)
      void library.importFiles(paths, playlist.id, before)
    } else {
      void library.importFiles(paths)
    }
  }

  function viewportDragOver(e: DragEvent): void {
    fileDragOver(e)
    autoScroll(e)
  }

  function autoScroll(e: DragEvent): void {
    if (!(library.dragging || fileDrop) || !library.canReorder || !viewport) return
    const rect = viewport.getBoundingClientRect()
    if (e.clientY < rect.top + ROW + AUTOSCROLL_EDGE) viewport.scrollTop -= AUTOSCROLL_STEP
    else if (e.clientY > rect.bottom - AUTOSCROLL_EDGE) viewport.scrollTop += AUTOSCROLL_STEP
  }
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={endPointer} onpointercancel={endPointer} />

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
      {@const group = duplicateView ? library.positions.get(track.id) : undefined}
      <span class={track.missing ? 'text-warn/70' : 'text-faint'} title={group ? `Group ${group}` : undefined}>
        {library.positions.get(track.id) ?? index + 1}
      </span>
    {/if}
  {:else if column.id === 'color'}
    {#if TRACK_COLORS[track.color]}
      <span class="block h-4 w-[3px] rounded-full" style:background={TRACK_COLORS[track.color].hex}></span>
    {/if}
  {:else if column.id === 'art'}
    {@const fill = library.browserArtwork === 'fill'}
    {#if track.hasArtwork}
      <img
        src={artworkUrl(track.id, fill ? 'm' : 's')}
        alt=""
        loading="lazy"
        decoding="async"
        draggable="false"
        class={fill
          ? 'absolute inset-0 size-full object-cover object-top'
          : 'size-6 rounded-[3px] bg-raised object-cover'}
      />
    {:else}
      <span
        class={['grid place-items-center bg-raised text-faint', fill ? 'absolute inset-0' : 'size-6 rounded-[3px]']}
      >
        <Music class={fill ? 'size-3.5' : 'size-3'} />
      </span>
    {/if}
  {:else if column.id === 'title'}
    <span class={['truncate font-medium', track.missing ? 'text-warn' : 'text-fg']}>{track.title}</span>
    {#if track.remixer}
      <span class={['ml-1.5 truncate', track.missing ? 'text-warn/70' : 'text-faint']}>{track.remixer}</span>
    {/if}
  {:else if column.id === 'bpm'}
    <span class={track.missing ? 'text-warn' : 'text-fg/90'}>{formatBpm(track.bpm)}</span>
  {:else if column.id === 'key'}
    <span class="font-medium" style:color={(!track.missing && keyColor(track.key)) || 'inherit'}>{track.key}</span>
  {:else if column.id === 'duration'}
    {formatDuration(track.duration)}
  {:else if column.id === 'wave'}
    {@const row = browserPreviews.preview(track.id)}
    <WaveformThumb {track} wave={row?.wave ?? null} cues={row?.cues ?? []} />
  {:else if column.id === 'cue'}
    {#if track.collected === false}
      <span class="text-faint" title="Not in the collection" aria-label="Not in the collection">
        <X class="size-3.5" />
      </span>
    {:else}
      {@const preview = browserPreviews.preview(track.id)}
      {@const labels = CUE_STARS.filter((star) => preview?.[star.key]).map((star) => star.label)}
      {#if labels.length}
        <span class="flex items-center gap-0.5" aria-label={labels.join(', ')} title={labels.join(', ')}>
          {#each CUE_STARS as star (star.key)}
            {#if preview?.[star.key]}
              <span class="inline-flex" style:color={star.color} aria-hidden="true">
                <Star class="size-3.5 fill-current" strokeWidth={0} />
              </span>
            {/if}
          {/each}
        </span>
      {/if}
    {/if}
  {:else if column.id === 'rating'}
    <span class="flex gap-px">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <Star
          class={['size-3', n <= track.rating ? 'fill-warn text-warn' : 'text-line']}
          strokeWidth={n <= track.rating ? 0 : 2}
        />
      {/each}
    </span>
  {:else if column.id === 'compat'}
    {@const verdict = compatibilityOf(track)}
    <span
      class="grid size-full place-items-center"
      role="img"
      aria-label={verdict.summary || verdict.problems[0] || 'Compatibility'}
      onmouseenter={(e) => showCompat(e.currentTarget, track.id)}
      onmouseleave={() => hideCompat(track.id)}
    >
      <span
        class={[
          'block size-2.5 rounded-full',
          verdict.color === 'green' && 'bg-ok',
          verdict.color === 'yellow' && 'bg-warn',
          verdict.color === 'red' && 'bg-danger'
        ]}
      ></span>
    </span>
  {:else if column.id === 'fileType'}
    <span
      class={[
        'rounded border px-1 py-px text-[10px] font-medium',
        track.missing ? 'border-warn/40 text-warn' : 'border-line text-faint'
      ]}
    >
      {track.fileType}
    </span>
  {:else}
    <span class="truncate">{track[column.id as keyof Track]}</span>
  {/if}
{/snippet}

<div class="relative h-full">
  <div
    bind:this={viewport}
    bind:clientHeight={height}
    class="tabular relative h-full overflow-auto"
    role="presentation"
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    ondragover={viewportDragOver}
    ondragleave={fileDragLeave}
    ondrop={fileDropped}
  >
    <div style:min-width="max(100%, {minWidth}px)">
      <div
        bind:this={headerEl}
        class="sticky top-0 z-10 grid h-8 cursor-grab items-center border-b border-line-soft bg-bg/95 text-[11px] font-medium text-faint backdrop-blur select-none"
        style:grid-template-columns={template}
        role="row"
        tabindex="-1"
        onpointerdown={startMove}
        oncontextmenu={openColumnMenu}
      >
        {#each tableColumns as column (column.id)}
          <div
            class={[
              'relative flex h-full min-w-0 items-center',
              moving?.active && moving.id === column.id && 'opacity-40'
            ]}
            data-column={column.id}
          >
            {#if column.sort && (column.sort !== 'position' || library.positions.size)}
              <button
                class={[
                  'flex h-full min-w-0 flex-1 cursor-grab items-center gap-1 px-2 hover:text-fg',
                  column.align === 'right' && 'flex-row-reverse',
                  column.align === 'center' && 'justify-center',
                  library.sort?.key === column.sort && 'text-fg'
                ]}
                onclick={() => {
                  if (suppressSort) return
                  library.toggleSort(column.sort!)
                }}
              >
                <span class="truncate">{headerLabel(column)}</span>
                {@render sortIcon(column.sort)}
              </button>
            {:else}
              <span
                class={[
                  'min-w-0 flex-1 truncate px-2',
                  column.align === 'right' && 'text-right',
                  column.align === 'center' && 'text-center'
                ]}
              >
                {headerLabel(column)}
              </span>
            {/if}
            {#if column.id !== 'issue'}
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
                    'pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2',
                    drag?.id === column.id ? 'bg-accent' : 'bg-faint/55 group-hover/resize:bg-muted'
                  ]}
                ></span>
              </div>
            {/if}
          </div>
        {/each}
        {#if moving?.active}
          <div
            class="pointer-events-none absolute inset-y-0 z-30 w-0.5 bg-accent"
            style:grid-column="1 / -1"
            style:grid-row="1"
            style:left="{moving.dropX}px"
          ></div>
        {/if}
      </div>

      <div
        bind:this={rowsEl}
        class="relative"
        style:height="{list.length * ROW}px"
        role="rowgroup"
        ondragover={rowsDragOver}
        ondragleave={rowsDragLeave}
        ondrop={rowsDrop}
      >
        <div class="absolute inset-x-0 top-0" style:transform="translateY({first * ROW}px)">
          {#each rows as track, i (track.id)}
            {@const index = first + i}
            {@const selected = library.selection.has(track.id)}
            <div
              class={[
                'grid cursor-default items-center text-[12.5px]',
                selected ? 'bg-select' : 'hover:bg-surface',
                track.missing ? 'text-warn' : selected ? 'text-fg' : 'text-muted',
                library.canReorder && dragged.has(track.id) && 'opacity-40'
              ]}
              style:grid-template-columns={template}
              style:height="{ROW}px"
              role="row"
              tabindex="-1"
              aria-selected={selected}
              title={track.missing ? 'Audio file not found. Right-click to relocate it.' : undefined}
              draggable={library.source.type !== 'device' && track.collected !== false}
              onmousedown={(e) => rowMousedown(e, track)}
              onmouseup={(e) => rowMouseup(e, track)}
              oncontextmenu={(e) => rowContextMenu(e, track)}
              ondragstart={(e) => dragStart(e, track)}
              ondragend={dragEnd}
              ondblclick={() => onactivate(track)}
            >
              {#each tableColumns as column (column.id)}
                <div
                  class={[
                    'flex min-w-0 items-center overflow-hidden px-2 whitespace-nowrap',
                    column.align === 'right' && 'justify-end',
                    column.align === 'center' && 'justify-center',
                    column.id === 'wave' && 'h-full px-1.5',
                    column.id === 'art' && library.browserArtwork === 'fill' && 'relative h-8 px-0'
                  ]}
                >
                  {@render cell(column, track, index)}
                </div>
              {/each}
            </div>
          {/each}
        </div>
        {#if dropIndex !== null}
          <div
            class="pointer-events-none absolute inset-x-0 z-10 h-0.5 rounded-full bg-accent"
            style:top="{dropIndex * ROW - 1}px"
          ></div>
        {/if}
      </div>

      {#if library.status === 'ready' && !library.sourceLoading && list.length === 0}
        <div class="absolute inset-x-0 top-24 text-center text-[13px] text-faint">
          {library.query ? `No tracks match “${library.query}”` : 'No tracks here'}
        </div>
      {/if}
    </div>
  </div>

  {#if fileDrop && dropIndex === null}
    <div
      class="pointer-events-none absolute inset-0 z-20 grid place-items-center bg-accent-soft/40 ring-2 ring-accent/70 ring-inset"
    >
      <span class="rounded-md bg-raised px-3 py-1.5 text-[12.5px] font-medium text-fg shadow-lg">
        Import into {library.sourceTitle}
      </span>
    </div>
  {/if}

  {#if drag || moving?.active}
    <div class={['fixed inset-0 z-50', drag ? 'cursor-col-resize' : 'cursor-grabbing']}></div>
  {/if}

  {#if rowMenu}
    <ContextMenu menu={rowMenu} onclose={() => (rowMenu = null)} />
  {/if}

  {#if deletingFiles}
    {@const files = deletingFiles}
    <ConfirmDialog
      title={files.length === 1 ? 'Delete file' : 'Delete files'}
      message={files.length === 1
        ? `Move “${files[0].title}” to the ${trashName}? This file is not in the collection.`
        : `Move ${files.length} files to the ${trashName}? They are not in the collection.`}
      confirmLabel={`Move to ${trashName}`}
      danger
      onconfirm={() => void trashFiles(files)}
      onclose={() => (deletingFiles = null)}
    />
  {/if}

  <div
    bind:this={ghost}
    class="pointer-events-none fixed -top-20 left-0 max-w-64 truncate rounded-md border border-line bg-raised px-2.5 py-1 text-[12px] font-medium text-fg"
    aria-hidden="true"
  >
    {ghostLabel}
  </div>

  {#if shownTip}
    {@const tipped = library.byId.get(shownTip.id)}
    {#if tipped}
      {@const verdict = compatibilityOf(tipped)}
      <div
        class="pointer-events-none fixed z-50 w-72 rounded-md border border-line bg-surface px-2.5 py-2 text-[11.5px] leading-snug text-muted shadow-lg shadow-black/40"
        style:top="{shownTip.top}px"
        style:left="{shownTip.left}px"
        style:transform={shownTip.above ? 'translateY(-100%)' : undefined}
      >
        {#if verdict.summary}
          <p class="text-fg">{verdict.summary}</p>
        {/if}
        {#if verdict.problems.length}
          <ul class={['list-disc space-y-0.5 pl-4', verdict.summary && 'mt-1.5']}>
            {#each verdict.problems as problem (problem)}
              <li>{problem}</li>
            {/each}
          </ul>
        {/if}
        {#if verdict.incompatible.length}
          <p class={verdict.summary || verdict.problems.length ? 'mt-1.5' : ''}>
            <span class="text-fg">Incompatible:</span>
            {verdict.incompatible.join(', ')}
          </p>
        {/if}
      </div>
    {/if}
  {/if}
</div>
