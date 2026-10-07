<script lang="ts">
  import { LoaderCircle, AlertTriangle } from '@lucide/svelte'
  import { setApp } from '$lib/context'
  import { Library } from '$lib/library.svelte'
  import { Player } from '$lib/player.svelte'
  import { Keybinds } from '$lib/keybinds.svelte'
  import { HOT_CUE_ACTIONS } from '$lib/keymap'
  import { HOT_CUE_KINDS } from '$lib/format'
  import type { Track } from '$lib/types'
  import TitleBar from '$lib/components/TitleBar.svelte'
  import Sidebar from '$lib/components/Sidebar.svelte'
  import TrackTable from '$lib/components/TrackTable.svelte'
  import Deck from '$lib/components/Deck.svelte'
  import Inspector from '$lib/components/Inspector.svelte'
  import StatusBar from '$lib/components/StatusBar.svelte'
  import TrackEditDialog from '$lib/components/TrackEditDialog.svelte'

  const library = new Library()
  const player = new Player()
  const keybinds = new Keybinds()
  setApp({ library, player, keybinds })

  const PANELS = {
    sidebar: { min: 180, max: 400, fallback: 240 },
    inspector: { min: 260, max: 480, fallback: 320 }
  } as const

  type Panel = keyof typeof PANELS

  function clampWidth(panel: Panel, width: number): number {
    const spec = PANELS[panel]
    return Math.min(spec.max, Math.max(spec.min, Math.round(width)))
  }

  function readWidth(panel: Panel): number {
    const n = Number(localStorage.getItem(`${panel}Width`))
    return Number.isFinite(n) ? clampWidth(panel, n) : PANELS[panel].fallback
  }

  let inspectorOpen = $state(localStorage.getItem('inspectorOpen') !== 'false')
  let sidebarWidth = $state(readWidth('sidebar'))
  let inspectorWidth = $state(readWidth('inspector'))
  let panelDrag = $state<{ side: Panel; startX: number; startW: number } | null>(null)

  function widthOf(panel: Panel): number {
    return panel === 'sidebar' ? sidebarWidth : inspectorWidth
  }

  function setWidth(panel: Panel, width: number): void {
    const next = clampWidth(panel, width)
    if (panel === 'sidebar') sidebarWidth = next
    else inspectorWidth = next
  }

  function startPanelResize(e: PointerEvent, side: Panel): void {
    if (e.button !== 0 || e.detail > 1) return
    e.preventDefault()
    e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    panelDrag = { side, startX: e.clientX, startW: widthOf(side) }
  }

  function onPanelMove(e: PointerEvent): void {
    const drag = panelDrag
    if (!drag) return
    const delta = e.clientX - drag.startX
    const signed = drag.side === 'sidebar' ? delta : -delta
    setWidth(drag.side, drag.startW + signed)
  }

  function endPanelResize(): void {
    const drag = panelDrag
    if (!drag) return
    panelDrag = null
    localStorage.setItem(`${drag.side}Width`, String(widthOf(drag.side)))
  }

  function resetPanel(side: Panel): void {
    setWidth(side, PANELS[side].fallback)
    localStorage.setItem(`${side}Width`, String(widthOf(side)))
  }

  let searchInput = $state<HTMLInputElement>()
  let table = $state<ReturnType<typeof TrackTable>>()
  let previewTimer: ReturnType<typeof setTimeout> | undefined

  library.load()
  library.onTracksUpdated((rows) => {
    const loaded = player.track
    const fresh = loaded && rows.find((t) => t.id === loaded.id)
    if (fresh) player.track = fresh
  })

  let editing = $state.raw<Track[] | null>(null)

  function edit(tracks: Track[]): void {
    if (!tracks.length) return
    if (!library.writable) library.notify('Unlock editing to change track info')
    else editing = tracks
  }

  function select(track: Track): void {
    library.selectOnly(track.id)
    if (player.playing) return
    clearTimeout(previewTimer)
    previewTimer = setTimeout(() => player.load(track), 80)
  }

  function activate(track: Track): void {
    library.selectOnly(track.id)
    clearTimeout(previewTimer)
    player.load(track, true)
  }

  function removeSelectedFromPlaylist(): void {
    const playlist = library.sourcePlaylist
    const ids = library.selectedTracks.map((t) => t.id)
    if (playlist?.kind !== 'playlist' || !ids.length) return
    const list = library.visible
    const last = Math.max(...ids.map((id) => list.findIndex((t) => t.id === id)))
    const removing = new Set(ids)
    const next = list.slice(last + 1).find((t) => !removing.has(t.id)) ?? list.findLast((t) => !removing.has(t.id))
    void library.removeFromPlaylist(playlist.id, ids).then(() => {
      if (next && library.selection.has(ids[0])) library.selectOnly(next.id)
    })
  }

  function reload(): void {
    library.load(true)
  }

  function moveSelection(step: number): void {
    const list = library.visible
    if (!list.length) return
    const current = library.selectedId ? list.findIndex((t) => t.id === library.selectedId) : -1
    const next = Math.max(0, Math.min(list.length - 1, current < 0 ? 0 : current + step))
    select(list[next])
    table?.ensureVisible(next)
  }

  const TEXT_INPUTS = new Set(['text', 'search', 'email', 'password', 'url'])

  function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false
    if (target.isContentEditable || target.tagName === 'TEXTAREA') return true
    return target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type)
  }

  function onmousedown(e: MouseEvent): void {
    if (!(e.target instanceof Element)) return
    if (e.target.closest('input, textarea, select, [contenteditable]')) return
    const active = document.activeElement
    if (active instanceof HTMLElement && isTypingTarget(active)) active.blur()
    if (e.target.closest('[draggable="true"]')) return
    if (e.target.closest('button, a, [tabindex]')) e.preventDefault()
  }

  function blurFocusedControl(): void {
    const active = document.activeElement
    if (active instanceof HTMLElement && active !== document.body && !isTypingTarget(active)) active.blur()
  }

  $effect(() =>
    keybinds.register({
      'deck.playPause': () => {
        blurFocusedControl()
        player.toggle()
      },
      'deck.cue': { down: () => player.cueDown(), up: () => player.cueUp() },
      'deck.loopIn': () => player.setLoopIn(),
      'deck.quantise': () => player.setQuantise(!player.quantise),
      'deck.seekBack': () => player.seek(player.position - 2),
      'deck.seekForward': () => player.seek(player.position + 2),
      'deck.seekBackFar': () => player.seek(player.position - 10),
      'deck.seekForwardFar': () => player.seek(player.position + 10),
      ...Object.fromEntries(HOT_CUE_ACTIONS.map((id, i) => [id, () => player.triggerHotCue(HOT_CUE_KINDS[i])])),
      'cue.previousMemory': () => player.callMemoryCue(-1),
      'cue.nextMemory': () => player.callMemoryCue(1),
      'cue.addMemory': () => void player.addMemoryCue(),
      'cue.deleteAtPlayhead': () => void player.deleteCueAtPlayhead(),
      'library.next': () => moveSelection(1),
      'library.previous': () => moveSelection(-1),
      'library.nextPage': () => moveSelection(10),
      'library.previousPage': () => moveSelection(-10),
      'library.loadSelected': () => {
        if (library.selected) activate(library.selected)
      },
      'library.search': () => {
        searchInput?.focus()
        searchInput?.select()
      },
      'library.reload': reload,
      'library.selectAll': () => library.selectAll(),
      'library.editInfo': () => edit(library.selectedTracks),
      'library.removeFromPlaylist': removeSelectedFromPlaylist
    })
  )
</script>

<svelte:window
  onkeydown={keybinds.keydown}
  onkeyup={keybinds.keyup}
  onblur={keybinds.releaseAll}
  {onmousedown}
  onpointermove={onPanelMove}
  onpointerup={endPanelResize}
  onpointercancel={endPanelResize}
/>

{#snippet panelHandle(side: 'sidebar' | 'inspector', edge: 'left' | 'right')}
  <div
    class={[
      'group/resize absolute inset-y-0 z-20 w-2 cursor-col-resize touch-none',
      edge === 'right' ? '-right-1' : '-left-1'
    ]}
    role="separator"
    aria-orientation="vertical"
    aria-valuenow={widthOf(side)}
    aria-valuemin={PANELS[side].min}
    aria-valuemax={PANELS[side].max}
    aria-label={side === 'sidebar' ? 'Resize playlists' : 'Resize info'}
    title="Drag to resize. Double-click to reset."
    onpointerdown={(e) => startPanelResize(e, side)}
    ondblclick={() => resetPanel(side)}
  >
    <span
      class={[
        'pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2',
        panelDrag?.side === side ? 'bg-accent' : 'bg-line opacity-0 group-hover/resize:opacity-100'
      ]}
    ></span>
  </div>
{/snippet}

<div class="flex h-full flex-col">
  <TitleBar
    bind:searchInput
    bind:inspectorOpen={
      () => inspectorOpen,
      (v) => {
        inspectorOpen = v
        localStorage.setItem('inspectorOpen', String(v))
      }
    }
  />

  {#if library.status === 'error'}
    <div class="grid flex-1 place-items-center">
      <div class="max-w-md text-center">
        <AlertTriangle class="mx-auto size-6 text-danger" />
        <p class="mt-3 text-[14px] font-medium">Couldn’t open your rekordbox library</p>
        <p class="mt-1 text-[12px] break-words text-faint">{library.error}</p>
        <button class="mt-4 rounded-md bg-raised px-3 py-1.5 text-[12px] hover:bg-hover" onclick={reload}>
          Try again
        </button>
      </div>
    </div>
  {:else if library.status !== 'ready' && !library.tracks.length}
    <div class="grid flex-1 place-items-center text-faint">
      <div class="flex items-center gap-2 text-[13px]">
        <LoaderCircle class="size-4 animate-spin" />
        Opening rekordbox library…
      </div>
    </div>
  {:else}
    <Deck />
    <div class="flex min-h-0 flex-1">
      <div class="relative h-full shrink-0" style:width="{sidebarWidth}px">
        <Sidebar
          onsource={() => {
            table?.scrollToTop()
          }}
        />
        {@render panelHandle('sidebar', 'right')}
      </div>
      <main class="min-w-0 flex-1 bg-bg">
        <TrackTable bind:this={table} onselect={select} onactivate={activate} onedit={edit} />
      </main>
      {#if inspectorOpen}
        <div class="relative h-full shrink-0" style:width="{inspectorWidth}px">
          {@render panelHandle('inspector', 'left')}
          <Inspector onjump={(s) => player.seek(s)} onedit={edit} />
        </div>
      {/if}
    </div>
    <StatusBar />
  {/if}

  {#if panelDrag}
    <div class="fixed inset-0 z-50 cursor-col-resize"></div>
  {/if}

  {#if editing}
    <TrackEditDialog tracks={editing} onclose={() => (editing = null)} />
  {/if}
</div>
