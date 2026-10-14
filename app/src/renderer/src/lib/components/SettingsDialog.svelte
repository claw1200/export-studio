<script module lang="ts">
  export type SettingsSection = 'general' | 'display' | 'analysis' | 'database' | 'keyboard' | 'developer'
</script>

<script lang="ts">
  import {
    ArchiveRestore,
    AudioWaveform,
    Database,
    DatabaseBackup,
    FolderOpen,
    Keyboard,
    LoaderCircle,
    Monitor,
    Plus,
    RotateCcw,
    Search,
    SlidersHorizontal,
    Trash2,
    Wrench,
    X
  } from '@lucide/svelte'
  import { accent, DEFAULT_ACCENT } from '$lib/accent.svelte'
  import { BPM_RANGES, MAX_CONCURRENCY } from '$lib/analysis.svelte'
  import { backupDate, KEEP_CHOICES } from '$lib/backups.svelte'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import { PLAYERS } from '$lib/hardware/players'
  import { formatSize } from '$lib/format'
  import { ACTION_IDS, ACTIONS, chordFromEvent, chordLabel, type ActionGroup, type ActionId } from '$lib/keymap'
  import type { BrowserArtwork } from '$lib/library.svelte'
  import type { CuePlacement } from '$lib/player.svelte'
  import type { Backup, BackupProgress, BackupReason } from '$lib/types'
  import type { GridMode } from '../../../../preload/analysis'
  import ConfirmDialog from './ConfirmDialog.svelte'

  let { initialSection = 'general', onclose }: { initialSection?: SettingsSection; onclose: () => void } = $props()

  const { library, player, keybinds, analysis, backups } = getApp()

  type Choice = { id: string; label: string }
  const SECTIONS: { id: SettingsSection; label: string; icon: typeof Keyboard }[] = [
    { id: 'general', label: 'General', icon: SlidersHorizontal },
    { id: 'display', label: 'Display', icon: Monitor },
    { id: 'analysis', label: 'Import & analysis', icon: AudioWaveform },
    { id: 'database', label: 'Database & files', icon: Database },
    { id: 'keyboard', label: 'Keyboard shortcuts', icon: Keyboard },
    { id: 'developer', label: 'Developer options', icon: Wrench }
  ]
  const REASONS: Record<BackupReason, string> = {
    manual: 'Manual',
    'before-edit': 'Before editing',
    daily: 'Daily',
    'pre-restore': 'Replaced by a restore'
  }
  const KEEP: Choice[] = KEEP_CHOICES.map((n) => ({ id: String(n), label: String(n) }))
  const GROUPS: ActionGroup[] = ['Playback', 'Cues', 'Library', 'App']
  const PLACEMENTS: { id: CuePlacement; label: string }[] = [
    { id: 'playhead', label: 'Playhead' },
    { id: 'cuePoint', label: 'Cue point' }
  ]
  const ARTWORK: (Choice & { id: BrowserArtwork })[] = [
    { id: 'fill', label: 'Fill' },
    { id: 'whole', label: 'Whole' }
  ]
  const GRID_MODES: (Choice & { id: GridMode })[] = [
    { id: 'normal', label: 'Normal' },
    { id: 'dynamic', label: 'Dynamic' }
  ]
  const RANGES: Choice[] = BPM_RANGES.map(([lo, hi]) => ({ id: `${lo}-${hi}`, label: `${lo}–${hi}` }))
  const CONCURRENCY: Choice[] = Array.from({ length: MAX_CONCURRENCY }, (_, i) => ({
    id: String(i + 1),
    label: String(i + 1)
  }))

  // svelte-ignore state_referenced_locally
  let section = $state<SettingsSection>(initialSection)
  let recording = $state<ActionId | null>(null)
  let confirming = $state.raw<{ kind: 'restore' | 'delete'; backup: Backup } | null>(null)

  void backups.refresh()

  function progressLabel(p: BackupProgress): string {
    const count = p.total ? ` ${p.done ?? 0} of ${p.total} files` : ''
    if (p.stage === 'checking') return p.reason === 'restore' ? 'Checking the backup…' : 'Checking the library…'
    if (p.stage === 'extracting') return `Restoring${count}`
    return `Saving${count}`
  }

  function restoreMessage(backup: Backup): string {
    const other = backup.otherLibrary ? ` It was made from a different library (${backup.dbPath}).` : ''
    return `Replaces your rekordbox library with the one from ${backupDate(backup)}.${other} Your current library is kept in the backups folder.`
  }
  let notice = $state<string | null>(null)
  let filter = $state('')
  let swallowKeyup: string | null = null

  const groups = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    const matches = (id: ActionId): boolean =>
      !q ||
      ACTIONS[id].label.toLowerCase().includes(q) ||
      keybinds.bindings[id].some((chord) => chordLabel(chord).toLowerCase().includes(q))
    return GROUPS.map((group) => ({
      group,
      ids: ACTION_IDS.filter((id) => ACTIONS[id].group === group && matches(id))
    })).filter((g) => g.ids.length)
  })

  function startRecording(id: ActionId): void {
    notice = null
    recording = recording === id ? null : id
  }

  function onkeydown(e: KeyboardEvent): void {
    if (!recording) return
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      recording = null
      return
    }
    const chord = chordFromEvent(e)
    if (!chord) return
    const id = recording
    recording = null
    swallowKeyup = e.code
    const owner = keybinds.addChord(id, chord)
    notice = owner ? `${chordLabel(chord)} was moved from “${ACTIONS[owner].label}” to “${ACTIONS[id].label}”.` : null
  }

  function onkeyup(e: KeyboardEvent): void {
    if (swallowKeyup !== e.code) return
    swallowKeyup = null
    e.preventDefault()
  }

  function resetAll(): void {
    recording = null
    notice = null
    keybinds.resetAll()
  }
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto h-[600px] max-h-[calc(100vh-48px)] w-[780px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Settings"
  {onkeydown}
  {onkeyup}
  oncancel={(e) => {
    if (!recording) return
    e.preventDefault()
    recording = null
  }}
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <div class="flex h-full">
    <nav class="flex w-48 shrink-0 flex-col gap-0.5 border-r border-line-soft bg-bg/50 p-3">
      <div class="px-2 pt-1 pb-3 text-[13px] font-semibold">Settings</div>
      {#each SECTIONS as s (s.id)}
        <button
          class={[
            'flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px]',
            section === s.id ? 'bg-raised text-fg' : 'text-muted hover:bg-hover hover:text-fg'
          ]}
          aria-current={section === s.id ? 'page' : undefined}
          onclick={() => {
            section = s.id
            recording = null
          }}
        >
          <s.icon class="size-3.5" />
          {s.label}
        </button>
      {/each}
    </nav>

    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
        <h2 class="text-[13px] font-semibold">{SECTIONS.find((s) => s.id === section)?.label}</h2>
        {#if section === 'keyboard'}
          <div class="relative ml-auto w-56">
            <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-faint" />
            <input
              bind:value={filter}
              type="search"
              placeholder="Filter shortcuts"
              spellcheck="false"
              class="h-7 w-full rounded-md border border-line bg-bg pr-2 pl-7 text-[12px] text-fg placeholder:text-faint focus:border-accent/60 [&::-webkit-search-cancel-button]:hidden"
            />
          </div>
        {/if}
        <button
          class={[
            'grid size-7 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg',
            section !== 'keyboard' && 'ml-auto'
          ]}
          onclick={(e) => e.currentTarget.closest('dialog')?.close()}
          aria-label="Close settings"
        >
          <X class="size-4" />
        </button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {#if section === 'developer'}
          <div class="flex items-center gap-3 rounded-md px-2 py-2">
            <span class="min-w-0 flex-1">
              <span class="block text-[12.5px] font-medium">Accent color</span>
              <span class="mt-0.5 block text-[11px] leading-snug text-faint"> Buttons, highlights, and progress. </span>
            </span>
            <input
              class="h-7 w-10 shrink-0 cursor-pointer overflow-hidden rounded-md border border-line bg-transparent p-0 [&::-webkit-color-swatch]:border-none [&::-webkit-color-swatch-wrapper]:p-0"
              type="color"
              aria-label="Accent color"
              value={accent.color}
              oninput={(e) => accent.set(e.currentTarget.value)}
              onchange={(e) => accent.set(e.currentTarget.value)}
            />
            <span class="w-16 shrink-0 text-right font-mono text-[11px] text-muted uppercase tabular-nums">
              {accent.color}
            </span>
            <button
              class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:invisible"
              disabled={accent.color === DEFAULT_ACCENT}
              onclick={() => accent.reset()}
              aria-label="Reset accent color"
              title="Reset to default"
            >
              <RotateCcw class="size-3" />
            </button>
          </div>
          <div class="flex items-center gap-3 rounded-md px-2 py-2">
            <span class="min-w-0 flex-1">
              <span class="block text-[12.5px] font-medium">Fall blocks</span>
              <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                How many blocks a 3-band fall slopes across. 0 keeps the steps.
              </span>
            </span>
            <input
              class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
              type="range"
              min="0"
              max="100"
              step="1"
              aria-label="Fall blocks"
              aria-valuetext={String(player.display.fallBlocks)}
              value={player.display.fallBlocks}
              oninput={(e) => player.display.setFallBlocks(Number(e.currentTarget.value))}
            />
            <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">
              {player.display.fallBlocks}
            </span>
          </div>
        {:else if section === 'display'}
          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Browser</h3>
            {@render choice(
              'Artwork',
              'Fill uses the full width of the column and crops the bottom of the cover. Whole shows the entire image, smaller.',
              ARTWORK,
              library.browserArtwork,
              (id) => library.setBrowserArtwork(id as BrowserArtwork)
            )}
            {@render toggle(
              'Hide streaming tracks',
              'Leave tracks from streaming services out of the collection, playlists, histories and tags. They stay in rekordbox.',
              library.hideStreaming,
              () => library.setHideStreaming(!library.hideStreaming)
            )}
            {@render toggle(
              'Hide default tracks',
              'Leave the sampler presets rekordbox installs, such as Groove Circuit, out of the collection, playlists, histories and tags. Samples you record stay listed. The presets stay in rekordbox.',
              library.hideDefaultTracks,
              () => library.setHideDefaultTracks(!library.hideDefaultTracks)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Hardware</h3>
            {@render playerMenu(
              'Target',
              'Green plays on this player and every newer one. Yellow plays here, but not on a newer player. Red will not play here.',
              library.hardwareGreen,
              (id) => library.setHardwareGreen(id)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Waveforms</h3>
            <div class="flex items-center gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">High quality waveforms</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Draw the detail waveform from the audio. Off uses the rekordbox analysis. Higher keeps more detail.
                </span>
              </span>
              <input
                class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
                type="range"
                min="0"
                max="100"
                step="1"
                aria-label="High quality waveforms"
                aria-valuetext={player.display.quality === 0 ? 'Off' : `${player.display.quality}%`}
                value={player.display.quality}
                oninput={(e) => player.display.setQuality(Number(e.currentTarget.value))}
              />
              <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">
                {player.display.quality === 0 ? 'Off' : `${player.display.quality}%`}
              </span>
            </div>
          </section>
        {:else if section === 'general'}
          <div class="flex flex-col gap-1">
            {@render toggle(
              'Quantise',
              'Scrubbing, the cue point and new cues snap to the nearest beat.',
              player.quantise,
              () => player.setQuantise(!player.quantise)
            )}
            <div class="flex items-start gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">Place new cues at</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Where new hot cues and memory cues go, from the buttons or the keyboard.
                </span>
              </span>
              <div
                class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[11px] font-semibold"
                role="radiogroup"
                aria-label="Place new cues at"
              >
                {#each PLACEMENTS as p (p.id)}
                  <button
                    class={[
                      'rounded px-2.5 py-1',
                      player.cuePlacement === p.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted'
                    ]}
                    role="radio"
                    aria-checked={player.cuePlacement === p.id}
                    onclick={() => player.setCuePlacement(p.id)}
                  >
                    {p.label}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {:else if section === 'database'}
          {@const rekordboxOpen = !!library.info?.rekordboxRunning}
          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Files</h3>
            {@render toggle(
              'Write track info to file tags',
              'When you edit title, artist, album and other text fields, also save them into the audio files’ own tags. Off keeps changes in rekordbox only. Rating and colour always stay in rekordbox.',
              library.writeFileTags,
              () => library.setWriteFileTags(!library.writeFileTags)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Backups</h3>
            {@render toggle(
              'Automatic backups',
              'Back up the library before the first edit each time you unlock editing, and once a day. Skipped when nothing has changed.',
              backups.settings.auto,
              () => void backups.setSettings({ auto: !backups.settings.auto })
            )}
            {@render choice(
              'Keep',
              'How many automatic backups to keep. Manual backups stay until you delete them.',
              KEEP,
              String(backups.settings.keep),
              (id) => void backups.setSettings({ keep: Number(id) })
            )}
            <p class="px-2 pt-1 text-[11px] leading-snug text-faint">
              Backups hold the rekordbox database, playlists, cues, beatgrids, waveforms and artwork. Audio files are
              not backed up.
            </p>
          </section>

          <div class="mb-3 flex items-center gap-2 px-2">
            <button
              class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 text-[12px] font-medium text-bg hover:bg-accent/90 disabled:pointer-events-none disabled:opacity-50"
              disabled={!!backups.busy}
              onclick={() => void backups.create()}
            >
              <DatabaseBackup class="size-3.5" />
              Back up now
            </button>
            <button
              class="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
              disabled={!backups.list.length}
              onclick={() => backups.list[0] && void window.api.showItemInFolder(backups.list[0].path)}
            >
              <FolderOpen class="size-3.5" />
              Show backups folder
            </button>
            {#if backups.progress}
              {@const p = backups.progress}
              <span class="ml-auto flex min-w-0 items-center gap-2 text-[11px] text-muted" role="status">
                <LoaderCircle class="size-3 shrink-0 animate-spin text-accent" />
                <span class="truncate">{progressLabel(p)}</span>
                {#if p.total}
                  <span class="h-1 w-20 shrink-0 overflow-hidden rounded-full bg-line">
                    <span
                      class="block h-full rounded-full bg-accent transition-[width]"
                      style:width="{(100 * (p.done ?? 0)) / p.total}%"
                    ></span>
                  </span>
                {/if}
              </span>
            {/if}
          </div>

          {#if backups.error}
            <p class="mx-2 mb-3 rounded-md border border-warn/60 bg-warn/10 px-2 py-1.5 text-[11.5px] text-warn">
              {backups.error}
            </p>
          {/if}

          <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Saved backups</h3>
          {#each backups.list as backup (backup.id)}
            <div class="group/row flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover/60">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">{backupDate(backup)}</span>
                <span class="mt-0.5 block truncate text-[11px] text-faint">
                  {REASONS[backup.reason] ?? backup.reason}
                  {#if backup.tracks != null}· {backup.tracks.toLocaleString()} tracks{/if}
                  · {formatSize(backup.size)}
                  {#if backup.otherLibrary}<span class="text-warn" title={backup.dbPath}> · other library</span>{/if}
                  {#if !backup.verified}<span title="This library was not checked when it was saved">
                      · unchecked</span
                    >{/if}
                </span>
              </span>
              <button
                class="flex h-6 items-center gap-1 rounded px-2 text-[11.5px] text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
                disabled={!!backups.busy || rekordboxOpen}
                title={rekordboxOpen ? 'Close rekordbox before restoring a backup' : 'Restore this backup'}
                onclick={() => (confirming = { kind: 'restore', backup })}
              >
                <ArchiveRestore class="size-3" />
                Restore
              </button>
              <button
                class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
                disabled={!!backups.busy}
                aria-label="Delete the backup from {backupDate(backup)}"
                title="Delete"
                onclick={() => (confirming = { kind: 'delete', backup })}
              >
                <Trash2 class="size-3" />
              </button>
            </div>
          {:else}
            <p class="px-2 text-[12px] text-faint">No backups yet.</p>
          {/each}
        {:else if section === 'analysis'}
          {@const s = analysis.settings}
          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Import</h3>
            {@render toggle(
              'Analyse imported tracks',
              'Work out the beatgrid, tempo and key of new tracks as soon as they are imported.',
              s.auto,
              () => analysis.set('auto', !s.auto)
            )}
            {@render toggle(
              'Import artwork',
              'Copy cover art embedded in the files into rekordbox.',
              library.importArtwork,
              () => library.setImportArtwork(!library.importArtwork)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Beatgrid</h3>
            {@render choice(
              'Grid',
              'Normal fits one tempo to the whole track. Dynamic follows tempo changes, for live recordings and tracks that speed up.',
              GRID_MODES,
              s.gridMode,
              (id) => analysis.set('gridMode', id as GridMode)
            )}
            {@render choice(
              'BPM range',
              'The tempo is always picked inside this range, so half- and double-tempo readings land where you expect.',
              RANGES,
              `${s.minBpm}-${s.maxBpm}`,
              (id) => {
                const [lo, hi] = id.split('-').map(Number)
                analysis.setRange(lo, hi)
              }
            )}
            {@render toggle(
              'High precision',
              'Put each beat on the start of the kick or snare, to the millisecond. Off places beats from the onset curve, which is a little less exact.',
              s.highPrecision,
              () => analysis.set('highPrecision', !s.highPrecision)
            )}
            {@render toggle(
              'Round BPM',
              'Snap the tempo to a whole number when it is within 0.05 BPM of one, as most produced music is.',
              s.roundBpm,
              () => analysis.set('roundBpm', !s.roundBpm)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Key and more</h3>
            {@render toggle('Detect key', 'Work out the musical key as well as the beatgrid.', s.key, () =>
              analysis.set('key', !s.key)
            )}
            {@render toggle(
              'Replace existing analysis',
              'When a selection mixes analysed and new tracks, replace the beatgrid, key and waveforms the analysed ones already have. Re-analyse always replaces them. Cues are always kept.',
              s.overwrite,
              () => analysis.set('overwrite', !s.overwrite)
            )}
            {@render choice(
              'Tracks analysed at once',
              'More is faster on machines with many cores, but leaves less for playback.',
              CONCURRENCY,
              String(s.concurrency),
              (id) => analysis.set('concurrency', Number(id))
            )}
          </section>

          <p class="px-2 text-[11px] leading-snug text-faint">
            This is Export Studio’s own analysis system, not rekordbox’s.
          </p>
        {:else}
          {#each groups as { group, ids } (group)}
            <section class="mb-5 last:mb-0">
              <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">{group}</h3>
              {#each ids as id (id)}
                {@render shortcutRow(id)}
              {/each}
            </section>
          {:else}
            <p class="px-2 text-[12px] text-faint">No shortcuts match “{filter}”.</p>
          {/each}
        {/if}
      </div>

      {#if section === 'keyboard'}
        <footer class="flex h-11 shrink-0 items-center gap-3 border-t border-line-soft px-5 text-[11.5px]">
          {#if recording}
            <span class="text-accent">Press a key combination. Esc cancels.</span>
          {:else if notice}
            <span class="truncate text-warn" title={notice}>{notice}</span>
          {:else}
            <span class="text-faint">Click + to add a shortcut. Hover a shortcut to remove it.</span>
          {/if}
          <button
            class="ml-auto flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
            disabled={!keybinds.customised}
            onclick={resetAll}
          >
            <RotateCcw class="size-3" />
            Reset all to defaults
          </button>
        </footer>
      {/if}
    </div>
  </div>
</dialog>

{#if confirming}
  {@const { kind, backup } = confirming}
  {#if kind === 'restore'}
    <ConfirmDialog
      title="Restore this backup?"
      message={restoreMessage(backup)}
      confirmLabel="Restore"
      danger
      onconfirm={() => void backups.restore(backup)}
      onclose={() => (confirming = null)}
    />
  {:else}
    <ConfirmDialog
      title="Delete this backup?"
      message="The backup from {backupDate(backup)} will be deleted. This can’t be undone."
      confirmLabel="Delete"
      danger
      onconfirm={() => void backups.remove(backup)}
      onclose={() => (confirming = null)}
    />
  {/if}
{/if}

{#snippet toggle(label: string, description: string, on: boolean, flip: () => void)}
  <button
    class="flex w-full items-start gap-3 rounded-md px-2 py-2 text-left hover:bg-hover"
    role="switch"
    aria-checked={on}
    onclick={flip}
  >
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <span
      class={['relative mt-0.5 h-[18px] w-8 shrink-0 rounded-full transition-colors', on ? 'bg-accent' : 'bg-line']}
      aria-hidden="true"
    >
      <span
        class={[
          'absolute top-0.5 left-0.5 size-3.5 rounded-full bg-white shadow transition-transform',
          on && 'translate-x-3.5'
        ]}
      ></span>
    </span>
  </button>
{/snippet}

{#snippet playerMenu(label: string, description: string, current: string, pick: (id: string) => void)}
  <label class="flex items-start gap-3 rounded-md px-2 py-2">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <select
      class="h-7 max-w-44 shrink-0 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
      aria-label={label}
      value={current}
      onchange={(e) => pick(e.currentTarget.value)}
    >
      {#each PLAYERS as player (player.id)}
        <option value={player.id}>{player.name}</option>
      {/each}
    </select>
  </label>
{/snippet}

{#snippet choice(label: string, description: string, options: Choice[], current: string, pick: (id: string) => void)}
  <div class="flex items-start gap-3 rounded-md px-2 py-2">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <div
      class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[11px] font-semibold tabular-nums"
      role="radiogroup"
      aria-label={label}
    >
      {#each options as o (o.id)}
        <button
          class={['rounded px-2 py-1', current === o.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted']}
          role="radio"
          aria-checked={current === o.id}
          onclick={() => pick(o.id)}
        >
          {o.label}
        </button>
      {/each}
    </div>
  </div>
{/snippet}

{#snippet shortcutRow(id: ActionId)}
  {@const spec = ACTIONS[id]}
  {@const chords = keybinds.bindings[id]}
  <div
    class={[
      'group/row flex min-h-9 items-center gap-3 rounded-md px-2 py-1',
      recording === id ? 'bg-accent-soft' : 'hover:bg-hover/60'
    ]}
  >
    <span class="min-w-0 flex-1 truncate text-[12.5px]">
      {spec.label}
      {#if 'hold' in spec && spec.hold}
        <span class="ml-1 text-[10.5px] text-faint">hold</span>
      {/if}
    </span>
    <div class="flex flex-wrap items-center justify-end gap-1">
      {#each chords as chord (chord)}
        <span
          class="group/chip relative flex h-6 items-center rounded border border-line bg-bg px-1.5 font-mono text-[11px] text-fg"
        >
          {chordLabel(chord)}
          <button
            class="ml-1 hidden size-4 place-items-center rounded text-faint group-hover/chip:grid hover:bg-hover hover:text-fg focus-visible:grid"
            onclick={() => keybinds.removeChord(id, chord)}
            aria-label="Remove {chordLabel(chord)} from {spec.label}"
            title="Remove"
          >
            <X class="size-3" />
          </button>
        </span>
      {/each}
      {#if recording === id}
        <span
          class="flex h-6 animate-pulse items-center rounded border border-accent/70 px-1.5 text-[11px] text-accent"
        >
          Press keys…
        </span>
      {:else if !chords.length}
        <span class="px-1 text-[11px] text-faint">Not set</span>
      {/if}
    </div>
    <div class="flex w-14 shrink-0 justify-end gap-0.5">
      <button
        class={[
          'grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg',
          recording === id && 'bg-accent/20 text-accent'
        ]}
        onclick={() => startRecording(id)}
        aria-label={recording === id ? 'Cancel adding a shortcut' : `Add a shortcut for ${spec.label}`}
        title={recording === id ? 'Cancel' : 'Add shortcut'}
      >
        <Plus class="size-3.5" />
      </button>
      <button
        class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:invisible"
        disabled={!keybinds.isCustomised(id)}
        onclick={() => keybinds.resetBinding(id)}
        aria-label="Reset {spec.label} to default"
        title="Reset to default"
      >
        <RotateCcw class="size-3" />
      </button>
    </div>
  </div>
{/snippet}
