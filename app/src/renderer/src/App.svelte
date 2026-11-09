<script lang="ts">
  import { LoaderCircle, AlertTriangle } from '@lucide/svelte'
  import { browserPreviews } from '$lib/browserPreview.svelte'
  import { setApp } from '$lib/context'
  import { Analysis, analysable } from '$lib/analysis.svelte'
  import { Backups } from '$lib/backups.svelte'
  import { Library } from '$lib/library.svelte'
  import { Decks } from '$lib/decks.svelte'
  import { MODES, modeById } from '$lib/modes/registry'
  import type { TrackActions } from '$lib/modes/types'
  import { Keybinds } from '$lib/keybinds.svelte'
  import { isFileDrag } from '$lib/library/drop'
  import { getSetting, setSetting } from '$lib/settings'
  import type { ConvertPreset, Track } from '$lib/types'
  import TitleBar from '$lib/components/TitleBar.svelte'
  import StatusBar from '$lib/components/StatusBar.svelte'
  import TrackEditDialog from '$lib/components/TrackEditDialog.svelte'
  import RelocateDialog from '$lib/components/RelocateDialog.svelte'
  import RemoveDialog from '$lib/components/RemoveDialog.svelte'
  import ConvertDialog from '$lib/components/ConvertDialog.svelte'
  import AnalysisDialog from '$lib/components/AnalysisDialog.svelte'
  import type { AnalysisSettings } from '$lib/analysis.svelte'
  import type { SettingsSection } from '$lib/components/SettingsDialog.svelte'

  const library = new Library()
  const decks = new Decks()
  const keybinds = new Keybinds()
  const analysis = new Analysis()
  const backups = new Backups(library, decks, analysis)
  setApp({ library, decks, keybinds, analysis, backups, audition: decks.audition })

  const initialMode = modeById(getSetting('mode'))
  let mode = $state.raw(initialMode)
  /** The players the current mode shows, claimed as it is entered. */
  let players = $state.raw(decks.enter(initialMode.decks))

  function setMode(id: string): void {
    const next = modeById(id)
    if (next === mode) return
    players = decks.enter(next.decks)
    mode = next
    setSetting('mode', next.id)
  }

  library.onImported((tracks) => analysis.afterImport(tracks))
  library.onRemoved((ids) => decks.ejectWhere((id) => ids.includes(id)))
  analysis.onFinished((track) => {
    if (!track) return
    browserPreviews.forget([track.id])
    library.updateRows([track])
    for (const player of decks.withTrack(track.id)) void player.refreshDetail()
  })
  analysis.onBatchDone((analysed, failed, error) => {
    if (!failed) return
    const count = failed === 1 ? 'one track' : `${failed} tracks`
    library.notify(`Analysed ${analysed}, but couldn’t analyse ${count}: ${error ?? 'unknown error'}`, 'warning')
  })

  let inspectorOpen = $state(getSetting('inspectorOpen') !== 'false')

  let searchInput = $state<HTMLInputElement>()

  browserPreviews.onCueChange = (id, cues) => library.noteCueCounts(id, cues)
  library.load()
  library.onTracksUpdated((rows) => {
    for (const player of decks.all) {
      const loaded = player.track
      const fresh = loaded && rows.find((t) => t.id === loaded.id)
      if (!fresh) continue
      if (fresh.path !== loaded.path) void player.reopen(fresh)
      else player.track = fresh
    }
  })

  decks.onFileChecked((id, missing) => library.setMissing(id, missing))

  let editing = $state.raw<Track[] | null>(null)
  let relocating = $state.raw<Track[] | null>(null)
  let removing = $state.raw<Track[] | null>(null)
  let converting = $state.raw<{ tracks: Track[]; preset: ConvertPreset } | null>(null)
  let analysing = $state.raw<Track[] | null>(null)
  let settings = $state<SettingsSection | null>(null)

  function collectionTracks(tracks: Track[]): Track[] {
    return tracks.filter((track) => track.collected !== false && !track.id.startsWith('usb:'))
  }

  function whenCollectionWritable(tracks: Track[], locked: string, act: (rows: Track[]) => void): void {
    const rows = collectionTracks(tracks)
    if (!rows.length) return
    if (!library.writable) library.notify(locked)
    else act(rows)
  }

  function remove(tracks: Track[]): void {
    whenCollectionWritable(tracks, 'Unlock editing to remove tracks', (rows) => {
      removing = rows
    })
  }

  function convert(tracks: Track[], preset: ConvertPreset): void {
    whenCollectionWritable(tracks, 'Unlock editing to convert tracks', (rows) => {
      converting = { tracks: rows, preset }
    })
  }

  function analyse(tracks: Track[]): void {
    whenCollectionWritable(tracks, 'Unlock editing to analyse tracks', (rows) => {
      const eligible = rows.filter(analysable)
      if (eligible.length) analysing = eligible
    })
  }

  function autoRelocate(tracks: Track[]): void {
    if (!tracks.length) return
    if (!library.writable) library.notify('Unlock editing to relocate tracks')
    else relocating = tracks
  }

  function edit(tracks: Track[]): void {
    if (tracks.some((track) => track.id.startsWith('usb:'))) {
      library.notify('Device tracks are read-only; sync to change them')
      return
    }
    whenCollectionWritable(tracks, 'Unlock editing to change track info', (rows) => {
      editing = rows
    })
  }

  function openInEdit(track: Track): void {
    const edit = modeById('edit')
    void decks.claim(edit.decks[0]).load(track)
    setMode(edit.id)
  }

  const actions: TrackActions = { edit, remove, analyse, convert, autoRelocate, openInEdit }

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
  }

  $effect(() =>
    keybinds.register({
      'library.search': () => {
        searchInput?.focus()
        searchInput?.select()
      },
      'library.reload': reload,
      'library.selectAll': () => library.selectAll(),
      'library.editInfo': () => edit(library.selectedTracks),
      'library.removeFromPlaylist': removeSelectedFromPlaylist,
      'library.removeFromCollection': () => remove(library.selectedTracks),
      'library.analyse': () => analyse(library.selectedTracks)
    })
  )
</script>

<svelte:window
  ondragover={(e) => {
    if (isFileDrag(e) && !e.defaultPrevented) {
      e.preventDefault()
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'none'
    }
  }}
  ondrop={(e) => {
    if (isFileDrag(e)) e.preventDefault()
  }}
  onkeydown={keybinds.keydown}
  onkeyup={keybinds.keyup}
  onblur={keybinds.releaseAll}
  {onmousedown}
/>

<div class="flex h-full flex-col">
  <TitleBar
    modes={MODES}
    mode={mode.id}
    onmode={setMode}
    bind:searchInput
    bind:settings
    inspectorToggle={mode.inspector}
    bind:inspectorOpen={
      () => inspectorOpen,
      (v) => {
        inspectorOpen = v
        setSetting('inspectorOpen', String(v))
      }
    }
  />

  {#if library.status === 'error'}
    <div class="grid flex-1 place-items-center">
      <div class="max-w-md text-center">
        <AlertTriangle class="mx-auto size-6 text-danger" />
        <p class="mt-3 text-[14px] font-medium">Couldn’t open your rekordbox library</p>
        <p class="mt-1 text-[12px] break-words text-faint">{library.error}</p>
        <div class="mt-4 flex justify-center gap-2">
          <button class="rounded-md bg-raised px-3 py-1.5 text-[12px] hover:bg-hover" onclick={reload}>
            Try again
          </button>
          <button
            class="rounded-md bg-raised px-3 py-1.5 text-[12px] hover:bg-hover"
            onclick={() => (settings = 'database')}
          >
            Restore from backup
          </button>
        </div>
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
    {#key mode}
      <mode.component {players} {actions} inspectorOpen={mode.inspector && inspectorOpen} />
    {/key}
    <StatusBar />
  {/if}

  {#if editing}
    <TrackEditDialog tracks={editing} onclose={() => (editing = null)} />
  {/if}

  {#if relocating}
    <RelocateDialog tracks={relocating} onclose={() => (relocating = null)} />
  {/if}

  {#if removing}
    <RemoveDialog tracks={removing} onclose={() => (removing = null)} />
  {/if}

  {#if converting}
    <ConvertDialog tracks={converting.tracks} preset={converting.preset} onclose={() => (converting = null)} />
  {/if}
  {#if analysing}
    <AnalysisDialog
      tracks={analysing}
      onrun={(tracks, options: AnalysisSettings) => analysis.analyse(tracks, options)}
      onclose={() => (analysing = null)}
    />
  {/if}
</div>
