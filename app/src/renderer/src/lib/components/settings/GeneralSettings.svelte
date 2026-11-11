<script lang="ts">
  import { RotateCcw } from '@lucide/svelte'
  import { api } from '$lib/api'
  import { DEFAULT_BACKUP_SETTINGS } from '$lib/backups.svelte'
  import { getApp } from '$lib/context'
  import { DEFAULT_DECK_PREFS, type CuePlacement, type TempoReadout } from '$lib/deck/prefs.svelte'
  import { layerModifierName } from '$lib/keymap'
  import { DEFAULT_LIBRARY_PREFS } from '$lib/library.svelte'
  import { resetSettings } from '$lib/settings'
  import ConfirmDialog from '../ConfirmDialog.svelte'
  import type { Choice } from './choice'
  import { allOf, setting } from './resettable'
  import SettingChoice from './SettingChoice.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingText from './SettingText.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { library, decks, keybinds, backups } = getApp()
  const prefs = decks.prefs

  const PLACEMENTS: Choice<CuePlacement>[] = [
    { id: 'playhead', label: 'Playhead' },
    { id: 'cuePoint', label: 'Cue point' }
  ]
  const TEMPO_READOUTS: Choice<TempoReadout>[] = [
    { id: 'absolute', label: 'Value' },
    { id: 'relative', label: 'Change' }
  ]

  const defaults = allOf(
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
  )

  let confirmingFactoryReset = $state(false)

  async function factoryReset(): Promise<void> {
    // Straight to the engine: backups.setSettings can re-list every backup, which only slows the quit.
    if (!backups.settingsAreDefault) await api.setBackupSettings(DEFAULT_BACKUP_SETTINGS).catch(() => {})
    resetSettings()
  }
</script>

<SettingsPage {title} {defaults}>
  <div class="flex flex-col gap-1">
    <SettingToggle
      label="Double-click to load"
      description="Single-click only selects a track. Double-click loads it and starts playback."
      on={library.doubleClickToLoad}
      onflip={() => library.setDoubleClickToLoad(!library.doubleClickToLoad)}
    />
    <SettingToggle
      label="Quantise"
      description="Scrubbing, the cue point and new cues snap to the nearest beat."
      on={prefs.quantise}
      onflip={() => prefs.setQuantise(!prefs.quantise)}
    />
    <SettingChoice
      label="Place new cues at"
      description="Where to place hotcues and memory cues."
      options={PLACEMENTS}
      current={prefs.cuePlacement}
      onpick={(p) => prefs.setCuePlacement(p)}
    />
    <SettingChoice
      label="BPM and key readout"
      description="Whether the deck show the current BPM and key, or the amount changed. Hold {layerModifierName(
        keybinds.layerModifier
      )} to switch."
      options={TEMPO_READOUTS}
      current={prefs.tempoReadout}
      onpick={(r) => prefs.setTempoReadout(r)}
    />
  </div>

  <SettingsGroup title="Reset">
    <div class="flex items-start gap-3 rounded-md px-2 py-2">
      <SettingText
        label="Factory settings"
        description="Put every setting, shortcut and layout back to how it was on first launch. Your rekordbox library and backups are not touched."
      />
      <button
        class="flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-danger/50 px-3 text-[12px] font-medium text-danger hover:bg-danger/10"
        onclick={() => (confirmingFactoryReset = true)}
      >
        <RotateCcw class="size-3.5" />
        Reset…
      </button>
    </div>
  </SettingsGroup>
</SettingsPage>

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
