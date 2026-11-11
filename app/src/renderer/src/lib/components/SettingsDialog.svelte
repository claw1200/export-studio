<script module lang="ts">
  export type SettingsSection =
    'general' | 'display' | 'theme' | 'mix' | 'analysis' | 'convert' | 'database' | 'keyboard' | 'developer' | 'about'
</script>

<script lang="ts">
  import {
    ArchiveRestore,
    Blend,
    AudioWaveform,
    Database,
    DatabaseBackup,
    FileAudio,
    ExternalLink,
    FolderOpen,
    Info,
    Keyboard,
    LoaderCircle,
    Monitor,
    Palette,
    Plus,
    RotateCcw,
    Search,
    SlidersHorizontal,
    Star,
    Trash2,
    Wrench,
    X
  } from '@lucide/svelte'
  import { accent, DEFAULT_ACCENT } from '$lib/accent.svelte'
  import { BPM_RANGES, MAX_CONCURRENCY } from '$lib/analysis.svelte'
  import { ROW_HEIGHT_MAX, ROW_HEIGHT_MIN, ROW_HEIGHT_STEP } from '$lib/browserColumns'
  import { backupDate, DEFAULT_BACKUP_SETTINGS, INTERVAL_CHOICES, KEEP_CHOICES } from '$lib/backups.svelte'
  import {
    BIT_DEPTHS,
    BITRATES,
    DEFAULT_PRESETS,
    FORMAT_NAMES,
    FORMATS,
    isLossless,
    presetKey,
    presetLabel,
    SAMPLE_RATES
  } from '$lib/convertPresets'
  import { loadPresets, savePresets } from '$lib/convertPresetStore'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { devOptions } from '$lib/devOptions.svelte'
  import { suspendModal } from '$lib/modal'
  import { resetSettings } from '$lib/settings'
  import { PLAYERS } from '$lib/hardware/players'
  import { MEMORY_CUE_LIMIT } from '$lib/player.svelte'
  import { BAND_NAMES_LOW_TO_HIGH, BLEND_MODES, bandStyle, DEFAULT_BAND_STYLE } from '$lib/waveform/bandStyle.svelte'
  import { DEFAULT_RGB_GRADIENT, rgbGradient } from '$lib/waveform/rgbGradient.svelte'
  import { formatSize } from '$lib/format'
  import {
    ACTION_IDS,
    ACTIONS,
    chordFromEvent,
    chordKeys,
    chordLabel,
    LAYER_MODIFIERS,
    layerModifierName,
    type ActionGroup,
    type ActionId,
    type LayerModifier
  } from '$lib/keymap'
  import { DEFAULT_LIBRARY_PREFS, type BrowserTrackStyle } from '$lib/library.svelte'
  import { DEFAULT_DECK_PREFS, type CuePlacement, type TempoReadout } from '$lib/deck/prefs.svelte'
  import {
    DEFAULT_CUSTOM_RGB_COLORS,
    DEFAULT_FALL_BLOCKS_PER_QUALITY,
    DEFAULT_QUALITIES,
    FALL_BLOCKS_PER_QUALITY_STEP,
    HEIGHT_SCALE_STEP,
    MAX_FALL_BLOCKS_PER_QUALITY,
    MAX_QUALITY,
    MIN_HEIGHT_SCALE,
    type WaveformView
  } from '$lib/deck/display.svelte'
  import type { WaveformMode } from '$lib/waveform/data'
  import type { Backup, BackupProgress, BackupReason, ConvertFormat, ConvertPreset } from '$lib/types'
  import type { GridMode } from '../../../../preload/analysis'
  import { REPO_URL, type Update } from '../../../../shared/updates'
  import appIcon from '../../../../../resources/icon.png'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import MixCurvePreview from './mix/MixCurvePreview.svelte'
  import {
    CURVES,
    DEFAULT_AUTO_MASTER,
    DEFAULT_CURVE_ID,
    DEFAULT_KEY_HIGHLIGHT_ID,
    DEFAULT_SWAP_TIMING_ID,
    KEY_HIGHLIGHTS,
    SWAP_TIMINGS,
    type CurveId,
    type KeyHighlightId,
    type SwapTimingId
  } from '$lib/mix/prefs.svelte'

  let { initialSection = 'general', onclose }: { initialSection?: SettingsSection; onclose: () => void } = $props()

  const { library, decks, keybinds, analysis, backups } = getApp()
  const prefs = decks.prefs
  const mixPrefs = decks.mixPrefs

  type Choice = { id: string; label: string }
  const PLAYER_CHOICES: Choice[] = PLAYERS.map((player) => ({ id: player.id, label: player.name }))
  const SECTIONS: { id: SettingsSection; label: string; icon: typeof Keyboard }[] = [
    { id: 'general', label: 'General', icon: SlidersHorizontal },
    { id: 'display', label: 'Display', icon: Monitor },
    { id: 'theme', label: 'Theme & colors', icon: Palette },
    { id: 'mix', label: 'Mix', icon: Blend },
    { id: 'analysis', label: 'Import & analysis', icon: AudioWaveform },
    { id: 'convert', label: 'Convert formats', icon: FileAudio },
    { id: 'database', label: 'Database & files', icon: Database },
    { id: 'keyboard', label: 'Keyboard shortcuts', icon: Keyboard },
    { id: 'developer', label: 'Developer options', icon: Wrench }
  ]
  const ABOUT = { id: 'about', label: 'About', icon: Info } as const
  const REASONS: Record<BackupReason, string> = {
    manual: 'Manual',
    'before-edit': 'Before editing',
    daily: 'Daily',
    'pre-restore': 'Replaced by a restore'
  }
  const KEEP: Choice[] = KEEP_CHOICES.map((n) => ({ id: String(n), label: String(n) }))
  const INTERVAL: Choice[] = INTERVAL_CHOICES.map((c) => ({ id: String(c.hours), label: c.label }))
  const GROUPS: ActionGroup[] = ['Playback', 'Cues', 'Beatgrid', 'Library', 'App']
  const PLACEMENTS: { id: CuePlacement; label: string }[] = [
    { id: 'playhead', label: 'Playhead' },
    { id: 'cuePoint', label: 'Cue point' }
  ]
  const TEMPO_READOUTS: (Choice & { id: TempoReadout })[] = [
    { id: 'absolute', label: 'Value' },
    { id: 'relative', label: 'Change' }
  ]
  const TRACK_STYLES: (Choice & { id: BrowserTrackStyle })[] = [
    { id: 'standard', label: 'Standard' },
    { id: 'alternative', label: 'Alternative' }
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

  let presets = $state<ConvertPreset[]>(loadPresets())
  let draftFormat = $state<ConvertFormat>('wav')
  let draftDepth = $state(16)
  let draftRate = $state(48000)
  let draftBitrate = $state(320)

  const draft = $derived<ConvertPreset>(
    isLossless(draftFormat)
      ? { format: draftFormat, bitDepth: draftDepth, sampleRate: draftRate }
      : { format: draftFormat, bitrate: draftBitrate }
  )
  const draftTaken = $derived(presets.some((p) => presetKey(p) === presetKey(draft)))

  function setPresets(next: ConvertPreset[]): void {
    presets = next
    savePresets(next)
  }

  // svelte-ignore state_referenced_locally
  let section = $state<SettingsSection>(initialSection)
  const sectionLabel = $derived([...SECTIONS, ABOUT].find((s) => s.id === section)?.label)

  let update = $state<Update | null | 'checking'>(null)
  let checkedForUpdate = $state(false)
  async function checkForUpdate(): Promise<void> {
    update = 'checking'
    update = await window.api.checkForUpdate()
    checkedForUpdate = true
  }
  let recording = $state<ActionId | null>(null)
  let confirming = $state.raw<{ kind: 'restore' | 'delete'; backup: Backup } | null>(null)
  let confirmingFactoryReset = $state(false)
  let confirmingDevOptions = $state<'on' | 'off' | null>(null)

  type Resettable = { isDefault: () => boolean; reset: () => void }

  function setting<T>(current: () => T, initial: T, set: (value: T) => void): Resettable {
    return { isDefault: () => current() === initial, reset: () => set(initial) }
  }

  function allOf(...settings: Resettable[]): Resettable {
    return {
      isDefault: () => settings.every((s) => s.isDefault()),
      reset: () => settings.forEach((s) => s.reset())
    }
  }

  const waveform = prefs.waveform
  const DEVELOPER_DEFAULTS = allOf(
    { isDefault: () => prefs.developerOptionsAreDefault, reset: () => prefs.resetDeveloperOptions() },
    { isDefault: () => bandStyle.layeringIsDefault, reset: () => bandStyle.resetLayering() }
  )
  const TAB_DEFAULTS: Partial<Record<SettingsSection, Resettable>> = {
    general: allOf(
      setting(
        () => library.doubleClickToLoad,
        DEFAULT_LIBRARY_PREFS.doubleClickToLoad,
        (on) => library.setDoubleClickToLoad(on)
      ),
      setting(
        () => prefs.quantise,
        DEFAULT_DECK_PREFS.quantise,
        (on) => prefs.setQuantise(on)
      ),
      setting(
        () => prefs.cuePlacement,
        DEFAULT_DECK_PREFS.cuePlacement,
        (p) => prefs.setCuePlacement(p)
      ),
      setting(
        () => prefs.tempoReadout,
        DEFAULT_DECK_PREFS.tempoReadout,
        (r) => prefs.setTempoReadout(r)
      )
    ),
    display: allOf(
      setting(
        () => library.browserTrackStyle,
        DEFAULT_LIBRARY_PREFS.browserTrackStyle,
        (style) => library.setBrowserTrackStyle(style)
      ),
      setting(
        () => library.browserRowHeight,
        DEFAULT_LIBRARY_PREFS.browserRowHeight,
        (height) => library.setBrowserRowHeight(height)
      ),
      setting(
        () => library.hideStreaming,
        DEFAULT_LIBRARY_PREFS.hideStreaming,
        (on) => library.setHideStreaming(on)
      ),
      setting(
        () => library.hidePresetTracks,
        DEFAULT_LIBRARY_PREFS.hidePresetTracks,
        (on) => library.setHidePresetTracks(on)
      ),
      setting(
        () => library.hardwareGreen,
        DEFAULT_LIBRARY_PREFS.hardwareGreen,
        (id) => library.setHardwareGreen(id)
      ),
      setting(
        () => waveform.qualities.rgb,
        DEFAULT_QUALITIES.rgb,
        (q) => waveform.setQuality('rgb', q)
      ),
      setting(
        () => waveform.qualities['3band'],
        DEFAULT_QUALITIES['3band'],
        (q) => waveform.setQuality('3band', q)
      ),
      setting(
        () => waveform.fallBlocksPerQuality,
        DEFAULT_FALL_BLOCKS_PER_QUALITY,
        (blocks) => waveform.setFallBlocksPerQuality(blocks)
      )
    ),
    theme: allOf(
      setting(
        () => accent.color,
        DEFAULT_ACCENT,
        (color) => accent.set(color)
      ),
      setting(
        () => waveform.customRgbColors,
        DEFAULT_CUSTOM_RGB_COLORS,
        (on) => waveform.setCustomRgbColors(on)
      ),
      { isDefault: () => rgbGradient.isDefault, reset: () => rgbGradient.reset() },
      { isDefault: () => bandStyle.colorsAreDefault, reset: () => bandStyle.resetColors() }
    ),
    mix: allOf(
      setting(
        () => mixPrefs.curveId,
        DEFAULT_CURVE_ID,
        (id) => mixPrefs.setCurve(id)
      ),
      setting(
        () => mixPrefs.swapTimingId,
        DEFAULT_SWAP_TIMING_ID,
        (id) => mixPrefs.setSwapTiming(id)
      ),
      setting(
        () => mixPrefs.autoMaster,
        DEFAULT_AUTO_MASTER,
        (on) => mixPrefs.setAutoMaster(on)
      ),
      setting(
        () => mixPrefs.keyHighlightId,
        DEFAULT_KEY_HIGHLIGHT_ID,
        (id) => mixPrefs.setKeyHighlight(id)
      )
    ),
    analysis: allOf(
      { isDefault: () => analysis.settingsAreDefault, reset: () => analysis.resetSettings() },
      setting(
        () => library.importArtwork,
        DEFAULT_LIBRARY_PREFS.importArtwork,
        (on) => library.setImportArtwork(on)
      )
    ),
    database: allOf(
      setting(
        () => library.writeFileTags,
        DEFAULT_LIBRARY_PREFS.writeFileTags,
        (on) => library.setWriteFileTags(on)
      ),
      { isDefault: () => backups.settingsAreDefault, reset: () => void backups.resetSettings() }
    ),
    convert: {
      isDefault: () => presets.map(presetKey).join() === DEFAULT_PRESETS.map(presetKey).join(),
      reset: () => setPresets(DEFAULT_PRESETS)
    },
    keyboard: { isDefault: () => !keybinds.customised, reset: () => resetKeybinds() },
    developer: DEVELOPER_DEFAULTS
  }
  const tabDefaults = $derived(TAB_DEFAULTS[section])

  function flipDevOptions(): void {
    if (!devOptions.enabled) confirmingDevOptions = 'on'
    else if (!DEVELOPER_DEFAULTS.isDefault()) confirmingDevOptions = 'off'
    else turnOffDevOptions()
  }
  function turnOffDevOptions(): void {
    devOptions.setEnabled(false)
    DEVELOPER_DEFAULTS.reset()
  }

  async function factoryReset(): Promise<void> {
    // Straight to the engine: backups.setSettings also re-lists every backup, which only slows the quit.
    if (!backups.settingsAreDefault) await api.setBackupSettings(DEFAULT_BACKUP_SETTINGS).catch(() => {})
    resetSettings()
  }

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

  function resetKeybinds(): void {
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
        {@render navItem(s)}
      {/each}
      <div class="mt-auto border-t border-line-soft pt-2">
        {@render navItem(ABOUT)}
      </div>
    </nav>

    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
        <h2 class="text-[13px] font-semibold">{sectionLabel}</h2>
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
        {#if section === 'about'}
          <div class="flex flex-col items-center px-2 pt-8 text-center">
            <img src={appIcon} alt="" class="size-20" />
            <h3 class="mt-3 text-[16px] font-semibold">Export Studio</h3>
            <p class="mt-0.5 text-[12px] text-muted">A modern library browser for rekordbox</p>
            <p class="mt-3 font-mono text-[11.5px] text-faint tabular-nums">
              Version {window.api.version} · Electron {window.api.electronVersion}
            </p>

            <div class="mt-5 flex h-7 items-center gap-2 text-[12px]">
              {#if update === 'checking'}
                <span class="flex items-center gap-1.5 text-muted" role="status">
                  <LoaderCircle class="size-3.5 animate-spin text-accent" />
                  Checking for updates…
                </span>
              {:else if update}
                <a
                  class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 font-medium text-bg hover:bg-accent/90"
                  href={update.url}
                  target="_blank"
                  rel="noreferrer"
                >
                  Download version {update.version}
                  <ExternalLink class="size-3.5" />
                </a>
              {:else}
                {#if checkedForUpdate}
                  <span class="text-muted">No newer version found.</span>
                {/if}
                <button
                  class="flex h-7 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg"
                  onclick={() => void checkForUpdate()}
                >
                  <RotateCcw class="size-3.5" />
                  {checkedForUpdate ? 'Check again' : 'Check for updates'}
                </button>
              {/if}
            </div>

            <div
              class="mt-6 flex w-full max-w-sm items-center gap-3 rounded-lg border border-line-soft bg-bg/50 p-3 text-left"
            >
              <Star class="size-5 shrink-0 text-accent" />
              <p class="min-w-0 flex-1 text-[11.5px] leading-snug text-muted">
                Find Export Studio useful? A star on GitHub helps other DJs find it.
              </p>
              <a
                class="flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-accent px-3 text-[12px] font-medium text-bg hover:bg-accent/90"
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
              >
                <Star class="size-3.5" />
                Star
              </a>
            </div>

            <div class="mt-4 flex gap-1 text-[12px]">
              {@render externalLink('Release notes', `${REPO_URL}/releases`)}
              {@render externalLink('Report a problem', `${REPO_URL}/issues`)}
            </div>
          </div>
        {:else if section === 'developer'}
          {@render toggle(
            'Enable developer options',
            'Settings for testing and troubleshooting.',
            devOptions.enabled,
            flipDevOptions
          )}
          <fieldset
            class="mt-3 min-w-0 border-t border-line-soft pt-3 disabled:opacity-40"
            disabled={!devOptions.enabled}
          >
            {@render toggle(
              'Snap playhead',
              'Draw the playhead exactly where the audio engine reports it, instead of easing small corrections. Can look less smooth.',
              prefs.snapPlayhead,
              () => prefs.setSnapPlayhead(!prefs.snapPlayhead)
            )}
            {@render toggle(
              'Unlimited memory cues',
              `Place more than ${MEMORY_CUE_LIMIT} memory cues on a track.`,
              prefs.unlimitedMemoryCues,
              () => prefs.setUnlimitedMemoryCues(!prefs.unlimitedMemoryCues)
            )}
            <section class="mt-5">
              <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Waveform height</h3>
              {@render heightScaleSlider('Player waveform', 'Height of the scrolling waveform on each deck.', 'detail')}
              {@render heightScaleSlider(
                'Overview waveforms',
                'Height of the whole-track waveform on each deck and in the browser.',
                'overview'
              )}
            </section>
            <section class="mt-5">
              <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Waveform colors</h3>
              {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
                {#if band > 0}
                  {@render slider(
                    `${name} opacity`,
                    `How much of the bands beneath show through the ${name.toLowerCase()} band where they overlap.`,
                    { min: 0, max: 1, step: 0.05 },
                    bandStyle.layeredOpacities[band],
                    `${Math.round(bandStyle.layeredOpacities[band] * 100)}%`,
                    (opacity) => bandStyle.setLayeredOpacity(band, opacity)
                  )}
                {/if}
              {/each}
              {@render menu(
                'Blend',
                'How each band mixes with the bands beneath it where they overlap.',
                BLEND_MODES,
                bandStyle.blend,
                (id) => bandStyle.setBlend(id)
              )}
            </section>
          </fieldset>
        {:else if section === 'theme'}
          {@render colorPicker(
            'Accent color',
            'Buttons, highlights, and progress.',
            accent.color,
            DEFAULT_ACCENT,
            (color) => accent.set(color)
          )}
          <section class="mt-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Color waveform</h3>
            {@render toggle(
              'Custom colors',
              'Color the waveform by sound, from low to high. Off uses the rekordbox colors.',
              prefs.waveform.customRgbColors,
              () => prefs.waveform.setCustomRgbColors(!prefs.waveform.customRgbColors)
            )}
            {#if prefs.waveform.customRgbColors}
              {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
                {@render colorPicker(
                  `${name} color`,
                  `The color for the ${name.toLowerCase()} band.`,
                  rgbGradient.stopsLowToHigh[band],
                  DEFAULT_RGB_GRADIENT[band],
                  (color) => rgbGradient.setStop(band, color)
                )}
              {/each}
            {/if}
          </section>
          <section class="mt-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">3-band waveform</h3>
            {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
              {@render colorPicker(
                `${name} color`,
                `Fill color for the ${name.toLowerCase()} band.`,
                bandStyle.colors[band],
                DEFAULT_BAND_STYLE.colors[band],
                (color) => bandStyle.setColor(band, color)
              )}
            {/each}
          </section>
        {:else if section === 'display'}
          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Browser</h3>
            {@render choice(
              'Track style',
              'Standard shows more tracks. Alternative combines the title and artist, but shows the full artwork.',
              TRACK_STYLES,
              library.browserTrackStyle,
              (id) => library.setBrowserTrackStyle(id as BrowserTrackStyle)
            )}
            {@render slider(
              'Track height',
              'Height of each track in the browser. Text, artwork and waveforms scale with it.',
              { min: ROW_HEIGHT_MIN, max: ROW_HEIGHT_MAX, step: ROW_HEIGHT_STEP },
              library.browserRowHeight,
              `${library.browserRowHeight}px`,
              (height) => library.setBrowserRowHeight(height)
            )}
            {@render toggle(
              'Hide streaming tracks',
              'Leave tracks from streaming services out of the collection, playlists, histories and tags. They stay in rekordbox.',
              library.hideStreaming,
              () => library.setHideStreaming(!library.hideStreaming)
            )}
            {@render toggle(
              'Hide preset tracks',
              'Leave the sampler presets rekordbox installs, such as Groove Circuit, out of the collection, playlists, histories and tags. Samples you record stay listed. The presets stay in rekordbox.',
              library.hidePresetTracks,
              () => library.setHidePresetTracks(!library.hidePresetTracks)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Hardware</h3>
            {@render menu(
              'Target',
              'Green plays on this player and every newer one. Yellow plays here, but not on a newer player. Red will not play here.',
              PLAYER_CHOICES,
              library.hardwareGreen,
              (id) => library.setHardwareGreen(id)
            )}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Waveforms</h3>
            {@render qualitySlider(
              'Color quality',
              'Draw the color detail waveform from the audio. Off uses the rekordbox analysis. Higher keeps more detail.',
              'rgb'
            )}
            {@render qualitySlider(
              '3-band quality',
              'Draw the 3-band detail waveform from the audio. Off uses the rekordbox analysis. Higher keeps more detail.',
              '3band'
            )}
            {@render slider(
              'Smoothing',
              'How gently the 3-band waveform eases down after each peak. Off shows hard steps.',
              { min: 0, max: MAX_FALL_BLOCKS_PER_QUALITY, step: FALL_BLOCKS_PER_QUALITY_STEP },
              prefs.waveform.fallBlocksPerQuality,
              prefs.waveform.fallBlocksPerQuality === 0 ? 'Off' : `${prefs.waveform.fallBlocksPerQuality.toFixed(2)}×`,
              (blocks) => prefs.waveform.setFallBlocksPerQuality(blocks)
            )}
          </section>
        {:else if section === 'mix'}
          <MixCurvePreview curve={mixPrefs.curve} range={mixPrefs.swapRange} />
          <div class="flex flex-col gap-1">
            {@render choice(
              'Crossfader curve',
              'How quickly a deck fades out as the fader moves away from it.',
              CURVES,
              mixPrefs.curveId,
              (id) => mixPrefs.setCurve(id as CurveId)
            )}
            {@render choice(
              'Bass swap',
              'When Swap bass is on, how soon the incoming deck takes the bass during a fade.',
              SWAP_TIMINGS,
              mixPrefs.swapTimingId,
              (id) => mixPrefs.setSwapTiming(id as SwapTimingId)
            )}
            {@render toggle(
              'Auto master',
              'The deck that alone has sound, playing and not faded out, becomes the master.',
              mixPrefs.autoMaster,
              () => mixPrefs.setAutoMaster(!mixPrefs.autoMaster)
            )}
            {@render choice(
              'Highlight matching keys',
              'Colours browser keys that mix with the master deck. Pick the loosest match to show.',
              KEY_HIGHLIGHTS,
              mixPrefs.keyHighlightId,
              (id) => mixPrefs.setKeyHighlight(id as KeyHighlightId)
            )}
          </div>
        {:else if section === 'general'}
          <div class="flex flex-col gap-1">
            {@render toggle(
              'Double-click to load',
              'Single-click only selects a track. Double-click loads it and starts playback.',
              library.doubleClickToLoad,
              () => library.setDoubleClickToLoad(!library.doubleClickToLoad)
            )}
            {@render toggle(
              'Quantise',
              'Scrubbing, the cue point and new cues snap to the nearest beat.',
              prefs.quantise,
              () => prefs.setQuantise(!prefs.quantise)
            )}
            <div class="flex items-start gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">Place new cues at</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Where to place hotcues and memory cues.
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
                      prefs.cuePlacement === p.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted'
                    ]}
                    role="radio"
                    aria-checked={prefs.cuePlacement === p.id}
                    onclick={() => prefs.setCuePlacement(p.id)}
                  >
                    {p.label}
                  </button>
                {/each}
              </div>
            </div>
            {@render choice(
              'BPM and key readout',
              `Whether the deck show the current BPM and key, or the amount changed. Hold ${layerModifierName(keybinds.layerModifier)} to switch.`,
              TEMPO_READOUTS,
              prefs.tempoReadout,
              (id) => prefs.setTempoReadout(id as TempoReadout)
            )}
          </div>

          <section class="mt-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Reset</h3>
            <div class="flex items-start gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">Factory settings</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Put every setting, shortcut and layout back to how it was on first launch. Your rekordbox library and
                  backups are not touched.
                </span>
              </span>
              <button
                class="flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-danger/50 px-3 text-[12px] font-medium text-danger hover:bg-danger/10"
                onclick={() => (confirmingFactoryReset = true)}
              >
                <RotateCcw class="size-3.5" />
                Reset…
              </button>
            </div>
          </section>
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
              'Back up the library before the first edit each time you unlock editing, and periodically while the app is open. Skipped when nothing has changed.',
              backups.settings.auto,
              () => void backups.setSettings({ auto: !backups.settings.auto })
            )}
            {@render choice(
              'Minimum time between backups',
              'Automatic backups are skipped if one was already taken within this time.',
              INTERVAL,
              String(backups.settings.interval),
              (id) => void backups.setSettings({ interval: Number(id) })
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
              'The tempo during track analysis is always picked inside this range.',
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
        {:else if section === 'convert'}
          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Formats</h3>
            <p class="mb-2 px-2 text-[11px] leading-snug text-faint">
              These appear under Convert file type in the track menu.
            </p>
            {#each presets as preset (presetKey(preset))}
              <div class="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover">
                <span class="min-w-0 flex-1 text-[12.5px]">{presetLabel(preset)}</span>
                <button
                  class="grid size-6 place-items-center rounded text-faint hover:bg-raised hover:text-danger"
                  aria-label={`Remove ${presetLabel(preset)}`}
                  onclick={() => setPresets(presets.filter((p) => presetKey(p) !== presetKey(preset)))}
                >
                  <Trash2 class="size-3.5" />
                </button>
              </div>
            {:else}
              <p class="px-2 text-[12px] text-faint">No formats. Add one below.</p>
            {/each}
          </section>

          <section class="mb-5">
            <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Add a format</h3>
            <div class="flex flex-wrap items-center gap-2 px-2 py-1">
              <select
                class="h-7 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
                aria-label="Format"
                bind:value={draftFormat}
              >
                {#each FORMATS as f (f)}
                  <option value={f}>{FORMAT_NAMES[f]}</option>
                {/each}
              </select>
              {#if isLossless(draftFormat)}
                <select
                  class="h-7 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
                  aria-label="Bit depth"
                  bind:value={draftDepth}
                >
                  {#each BIT_DEPTHS as d (d)}
                    <option value={d}>{d}-bit</option>
                  {/each}
                </select>
                <select
                  class="h-7 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
                  aria-label="Sample rate"
                  bind:value={draftRate}
                >
                  {#each SAMPLE_RATES as r (r)}
                    <option value={r}>{r / 1000} kHz</option>
                  {/each}
                </select>
              {:else}
                <select
                  class="h-7 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
                  aria-label="Bitrate"
                  bind:value={draftBitrate}
                >
                  {#each BITRATES as b (b)}
                    <option value={b}>{b} kbps</option>
                  {/each}
                </select>
              {/if}
              <button
                class="flex h-7 items-center gap-1 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
                disabled={draftTaken}
                onclick={() => setPresets([...presets, draft])}
              >
                <Plus class="size-3.5" /> Add
              </button>
            </div>
          </section>
        {:else}
          <div class="mb-4 flex items-start gap-3 rounded-md px-2 py-2">
            <span class="min-w-0 flex-1">
              <span class="block text-[12.5px] font-medium">Layer modifier</span>
              <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                In mix mode the deck shortcuts control deck A. Hold this key with them to control deck B. It also swaps
                what the BPM and key steppers show.
              </span>
            </span>
            <select
              class="h-7 shrink-0 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
              aria-label="Layer modifier"
              value={keybinds.layerModifier}
              onchange={(e) => keybinds.setLayerModifier(e.currentTarget.value as LayerModifier)}
            >
              {#each LAYER_MODIFIERS as modifier (modifier)}
                <option value={modifier}>{layerModifierName(modifier)}</option>
              {/each}
            </select>
          </div>
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

      {#if tabDefaults}
        <footer class="flex h-11 shrink-0 items-center gap-3 border-t border-line-soft px-5 text-[11.5px]">
          {#if section === 'keyboard'}
            {#if recording}
              <span class="text-accent">Press a key combination. Esc cancels.</span>
            {:else if notice}
              <span class="truncate text-warn" title={notice}>{notice}</span>
            {:else}
              <span class="text-faint">Click + to add a shortcut. Hover a shortcut to remove it.</span>
            {/if}
          {/if}
          <button
            class="ml-auto flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
            disabled={tabDefaults.isDefault()}
            onclick={tabDefaults.reset}
          >
            <RotateCcw class="size-3" />
            Reset to defaults
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

{#if confirmingFactoryReset}
  <ConfirmDialog
    title="Restart to reset settings"
    message="Quit to reset all settings to their defaults. Your rekordbox library and backups are kept."
    confirmLabel="Quit"
    danger
    onconfirm={() => void factoryReset()}
    onclose={() => (confirmingFactoryReset = false)}
  />
{/if}

{#if confirmingDevOptions === 'on'}
  <ConfirmDialog
    title="Turn on developer options?"
    message="Developer options are for testing and troubleshooting, and may allow you to cause damage to your library if you are not careful."
    confirmLabel="Turn on"
    danger
    onconfirm={() => devOptions.setEnabled(true)}
    onclose={() => (confirmingDevOptions = null)}
  />
{:else if confirmingDevOptions === 'off'}
  <ConfirmDialog
    title="Reset developer options?"
    message="Turning off developer options will reset all settings in this category to their defaults."
    confirmLabel="Turn off and reset"
    danger
    onconfirm={turnOffDevOptions}
    onclose={() => (confirmingDevOptions = null)}
  />
{/if}

{#snippet navItem(s: { id: SettingsSection; label: string; icon: typeof Keyboard })}
  <button
    class={[
      'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px]',
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
{/snippet}

{#snippet externalLink(label: string, href: string)}
  <a
    class="flex h-7 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg"
    {href}
    target="_blank"
    rel="noreferrer"
  >
    {label}
    <ExternalLink class="size-3" />
  </a>
{/snippet}

{#snippet slider(
  label: string,
  description: string,
  range: { min: number; max: number; step: number },
  value: number,
  valueText: string,
  set: (value: number) => void
)}
  <div class="flex items-center gap-3 rounded-md px-2 py-2">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <input
      class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
      type="range"
      min={range.min}
      max={range.max}
      step={range.step}
      aria-label={label}
      aria-valuetext={valueText}
      {value}
      oninput={(e) => set(Number(e.currentTarget.value))}
    />
    <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">{valueText}</span>
  </div>
{/snippet}

{#snippet qualitySlider(label: string, description: string, mode: WaveformMode)}
  {@const quality = prefs.waveform.qualities[mode]}
  {@render slider(
    label,
    description,
    { min: 0, max: MAX_QUALITY[mode], step: 1 },
    quality,
    quality === 0 ? 'Off' : `${quality}%`,
    (q) => prefs.waveform.setQuality(mode, q)
  )}
{/snippet}

{#snippet heightScaleSlider(label: string, description: string, view: WaveformView)}
  {@const scale = prefs.waveform.heightScales[view]}
  {@render slider(
    label,
    description,
    { min: MIN_HEIGHT_SCALE, max: 1, step: HEIGHT_SCALE_STEP },
    scale,
    `${Math.round(scale * 100)}%`,
    (s) => prefs.waveform.setHeightScale(view, s)
  )}
{/snippet}

{#snippet toggle(label: string, description: string, on: boolean, flip: () => void)}
  <button
    class="flex w-full items-start gap-3 rounded-md px-2 py-2 text-left enabled:hover:bg-hover"
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

{#snippet menu(label: string, description: string, options: Choice[], current: string, pick: (id: string) => void)}
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
      {#each options as o (o.id)}
        <option value={o.id}>{o.label}</option>
      {/each}
    </select>
  </label>
{/snippet}

{#snippet colorPicker(
  label: string,
  description: string,
  color: string,
  defaultColor: string,
  set: (color: string) => void
)}
  <div class="flex items-center gap-3 rounded-md px-2 py-2">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <input
      class="h-7 w-10 shrink-0 cursor-pointer overflow-hidden rounded-md border border-line bg-transparent p-0 [&::-webkit-color-swatch]:border-none [&::-webkit-color-swatch-wrapper]:p-0"
      type="color"
      aria-label={label}
      value={color}
      oninput={(e) => set(e.currentTarget.value)}
      onchange={(e) => set(e.currentTarget.value)}
    />
    <span class="w-16 shrink-0 text-right font-mono text-[11px] text-muted uppercase tabular-nums">{color}</span>
    <button
      class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:invisible"
      disabled={color === defaultColor}
      onclick={() => set(defaultColor)}
      aria-label="Reset {label.toLowerCase()}"
      title="Reset to default"
    >
      <RotateCcw class="size-3" />
    </button>
  </div>
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
      {#each chords as chord, i (chord)}
        {#if i > 0}
          <span class="px-0.5 text-[11px] text-faint">or</span>
        {/if}
        <span class="group/chip flex items-center gap-1 rounded-md py-0.5 pl-1 hover:bg-hover">
          {#each chordKeys(chord) as key, k (k)}
            {#if k > 0}
              <span class="text-[11px] text-faint" aria-hidden="true">+</span>
            {/if}
            <kbd
              class="grid h-6 min-w-6 place-items-center rounded-md border border-b-2 border-line bg-bg px-1.5 font-sans text-[12px] font-medium text-fg"
            >
              {key}
            </kbd>
          {/each}
          <button
            class="invisible grid size-5 place-items-center rounded text-faint group-hover/chip:visible hover:bg-raised hover:text-fg focus-visible:visible"
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
          class="flex h-7 animate-pulse items-center rounded-md border border-accent/70 px-2 text-[12px] text-accent"
        >
          Press keys…
        </span>
      {:else if !chords.length}
        <span class="px-1 text-[12px] text-faint">Not set</span>
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
