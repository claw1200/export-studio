<script lang="ts">
  import { getApp } from '$lib/context'
  import type { TrackActions } from '$lib/modes/types'
  import { getSetting, setSetting } from '$lib/settings'
  import type { Track } from '$lib/types'
  import Sidebar from './Sidebar.svelte'
  import TrackTable from './TrackTable.svelte'
  import Inspector from './Inspector.svelte'

  /**
   * Playlists, the track list and the inspector, with resizable panels and the keys that move
   * through rows. What a row does when picked is up to the mode: `onselect` follows a single click
   * or arrow key, `onactivate` a double click or the load shortcut.
   */
  let {
    actions,
    inspectorOpen,
    onselect,
    onactivate,
    settingsPrefix = ''
  }: {
    actions: TrackActions
    inspectorOpen: boolean
    onselect?: (track: Track) => void
    onactivate?: (track: Track) => void
    /** Keeps this browser's panel widths apart from another mode's. */
    settingsPrefix?: string
  } = $props()

  const { library, keybinds } = getApp()

  const PANELS = {
    sidebar: { min: 180, max: 400, fallback: 240 },
    inspector: { min: 260, max: 480, fallback: 320 }
  } as const

  type Panel = keyof typeof PANELS

  function widthKey(panel: Panel): string {
    return `${settingsPrefix}${panel}Width`
  }

  function clampWidth(panel: Panel, width: number): number {
    const spec = PANELS[panel]
    return Math.min(spec.max, Math.max(spec.min, Math.round(width)))
  }

  function readWidth(panel: Panel): number {
    const n = Number(getSetting(widthKey(panel)))
    return Number.isFinite(n) ? clampWidth(panel, n) : PANELS[panel].fallback
  }

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
    setSetting(widthKey(drag.side), String(widthOf(drag.side)))
  }

  function resetPanel(side: Panel): void {
    setWidth(side, PANELS[side].fallback)
    setSetting(widthKey(side), String(widthOf(side)))
  }

  let table = $state<ReturnType<typeof TrackTable>>()

  function select(track: Track): void {
    library.selectOnly(track.id)
    onselect?.(track)
  }

  function activate(track: Track): void {
    library.selectOnly(track.id)
    onactivate?.(track)
  }

  function moveSelection(step: number): void {
    const list = library.visible
    if (!list.length) return
    const current = library.selectedId ? list.findIndex((t) => t.id === library.selectedId) : -1
    const next = Math.max(0, Math.min(list.length - 1, current < 0 ? 0 : current + step))
    select(list[next])
    table?.ensureVisible(next)
  }

  $effect(() =>
    keybinds.register({
      'library.next': () => moveSelection(1),
      'library.previous': () => moveSelection(-1),
      'library.nextPage': () => moveSelection(10),
      'library.previousPage': () => moveSelection(-10),
      'library.loadSelected': () => {
        if (library.selected) activate(library.selected)
      }
    })
  )
</script>

<svelte:window onpointermove={onPanelMove} onpointerup={endPanelResize} onpointercancel={endPanelResize} />

{#snippet panelHandle(side: Panel, edge: 'left' | 'right')}
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
    <TrackTable
      bind:this={table}
      onselect={select}
      onactivate={activate}
      onedit={actions.edit}
      onautorelocate={actions.autoRelocate}
      onremove={actions.remove}
      onanalyse={actions.analyse}
      onconvert={actions.convert}
    />
  </main>
  {#if inspectorOpen}
    <div class="relative h-full shrink-0" style:width="{inspectorWidth}px">
      {@render panelHandle('inspector', 'left')}
      <Inspector onedit={actions.edit} />
    </div>
  {/if}
</div>

{#if panelDrag}
  <div class="fixed inset-0 z-50 cursor-col-resize"></div>
{/if}
