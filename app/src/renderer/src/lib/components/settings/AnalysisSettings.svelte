<script lang="ts">
  import { BPM_RANGES, MAX_CONCURRENCY } from '$lib/analysis.svelte'
  import { getApp } from '$lib/context'
  import { DEFAULT_LIBRARY_PREFS } from '$lib/library.svelte'
  import type { GridMode } from '../../../../../preload/analysis'
  import type { Choice } from './choice'
  import { allOf, setting } from './resettable'
  import SettingChoice from './SettingChoice.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { library, analysis } = getApp()
  const s = $derived(analysis.settings)

  const GRID_MODES: Choice<GridMode>[] = [
    { id: 'normal', label: 'Normal' },
    { id: 'dynamic', label: 'Dynamic' }
  ]
  const RANGES: Choice[] = BPM_RANGES.map(([lo, hi]) => ({ id: `${lo}-${hi}`, label: `${lo}–${hi}` }))
  const CONCURRENCY: Choice[] = Array.from({ length: MAX_CONCURRENCY }, (_, i) => ({
    id: String(i + 1),
    label: String(i + 1)
  }))

  const defaults = allOf(
    { isDefault: () => analysis.settingsAreDefault, reset: () => analysis.resetSettings() },
    setting(
      () => library.importArtwork,
      DEFAULT_LIBRARY_PREFS.importArtwork,
      (on) => library.setImportArtwork(on)
    )
  )

  function setRange(id: string): void {
    const [lo, hi] = id.split('-').map(Number)
    analysis.setRange(lo, hi)
  }
</script>

<SettingsPage {title} {defaults}>
  <SettingsGroup title="Import">
    <SettingToggle
      label="Analyse imported tracks"
      description="Work out the beatgrid, tempo and key of new tracks as soon as they are imported."
      on={s.auto}
      onflip={() => analysis.set('auto', !s.auto)}
    />
    <SettingToggle
      label="Import artwork"
      description="Copy cover art embedded in the files into rekordbox."
      on={library.importArtwork}
      onflip={() => library.setImportArtwork(!library.importArtwork)}
    />
  </SettingsGroup>

  <SettingsGroup title="Beatgrid">
    <SettingChoice
      label="Grid"
      description="Normal fits one tempo to the whole track. Dynamic follows tempo changes, for live recordings and tracks that speed up."
      options={GRID_MODES}
      current={s.gridMode}
      onpick={(mode) => analysis.set('gridMode', mode)}
    />
    <SettingChoice
      label="BPM range"
      description="The tempo during track analysis is always picked inside this range."
      options={RANGES}
      current="{s.minBpm}-{s.maxBpm}"
      onpick={setRange}
    />
    <SettingToggle
      label="High precision"
      description="Put each beat on the start of the kick or snare, to the millisecond. Off places beats from the onset curve, which is a little less exact."
      on={s.highPrecision}
      onflip={() => analysis.set('highPrecision', !s.highPrecision)}
    />
    <SettingToggle
      label="Round BPM"
      description="Snap the tempo to a whole number when it is within 0.05 BPM of one, as most produced music is."
      on={s.roundBpm}
      onflip={() => analysis.set('roundBpm', !s.roundBpm)}
    />
  </SettingsGroup>

  <SettingsGroup title="Key and more">
    <SettingToggle
      label="Detect key"
      description="Work out the musical key as well as the beatgrid."
      on={s.key}
      onflip={() => analysis.set('key', !s.key)}
    />
    <SettingChoice
      label="Tracks analysed at once"
      description="More is faster on machines with many cores, but leaves less for playback."
      options={CONCURRENCY}
      current={String(s.concurrency)}
      onpick={(id) => analysis.set('concurrency', Number(id))}
    />
  </SettingsGroup>

  <p class="mt-5 px-2 text-[11px] leading-snug text-faint">
    This is Export Studio’s own analysis system, not rekordbox’s.
  </p>
</SettingsPage>
