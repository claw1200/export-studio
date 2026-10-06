<script lang="ts">
  import { LoaderCircle, AlertTriangle } from '@lucide/svelte'
  import { setApp } from '$lib/context'
  import { Library } from '$lib/library.svelte'
  import { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import TitleBar from '$lib/components/TitleBar.svelte'
  import Sidebar from '$lib/components/Sidebar.svelte'
  import TrackTable from '$lib/components/TrackTable.svelte'
  import Deck from '$lib/components/Deck.svelte'
  import Inspector from '$lib/components/Inspector.svelte'
  import StatusBar from '$lib/components/StatusBar.svelte'

  const library = new Library()
  const player = new Player()
  setApp({ library, player })

  let inspectorOpen = $state(localStorage.getItem('inspectorOpen') !== 'false')
  let searchInput = $state<HTMLInputElement>()
  let table = $state<ReturnType<typeof TrackTable>>()
  let previewTimer: ReturnType<typeof setTimeout> | undefined

  library.load()

  function select(track: Track): void {
    library.selectedId = track.id
    if (player.playing) return
    clearTimeout(previewTimer)
    previewTimer = setTimeout(() => player.load(track), 80)
  }

  function activate(track: Track): void {
    library.selectedId = track.id
    clearTimeout(previewTimer)
    player.load(track, true)
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

  function onkeydown(e: KeyboardEvent): void {
    const mod = e.metaKey || e.ctrlKey
    if (mod && e.key.toLowerCase() === 'f') {
      e.preventDefault()
      searchInput?.focus()
      searchInput?.select()
      return
    }
    if (mod && e.key.toLowerCase() === 'r') {
      e.preventDefault()
      reload()
      return
    }
    const target = e.target as HTMLElement
    if (target.tagName === 'INPUT' && e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Enter') return

    switch (e.key) {
      case ' ':
        e.preventDefault()
        player.toggle()
        break
      case 'ArrowDown':
        e.preventDefault()
        moveSelection(e.shiftKey ? 10 : 1)
        break
      case 'ArrowUp':
        e.preventDefault()
        moveSelection(e.shiftKey ? -10 : -1)
        break
      case 'Enter':
        if (library.selected) activate(library.selected)
        break
      case 'ArrowLeft':
        e.preventDefault()
        player.seek(player.position - (e.shiftKey ? 10 : 2))
        break
      case 'ArrowRight':
        e.preventDefault()
        player.seek(player.position + (e.shiftKey ? 10 : 2))
        break
    }
  }
</script>

<svelte:window {onkeydown} />

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
      <Sidebar
        onsource={() => {
          table?.scrollToTop()
        }}
      />
      <main class="min-w-0 flex-1 bg-bg">
        <TrackTable bind:this={table} onselect={select} onactivate={activate} />
      </main>
      {#if inspectorOpen}
        <Inspector onjump={(s) => player.seek(s)} />
      {/if}
    </div>
    <StatusBar />
  {/if}
</div>
