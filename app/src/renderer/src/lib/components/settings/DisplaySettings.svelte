<script lang="ts">
  import { ROW_HEIGHT_MAX, ROW_HEIGHT_MIN, ROW_HEIGHT_STEP } from '$lib/browserColumns'
  import { getApp } from '$lib/context'
  import {
    DEFAULT_FALL_BLOCKS_PER_QUALITY,
    DEFAULT_QUALITIES,
    FALL_BLOCKS_PER_QUALITY_STEP,
    MAX_FALL_BLOCKS_PER_QUALITY,
    MAX_QUALITY
  } from '$lib/deck/display.svelte'
  import { PLAYERS } from '$lib/hardware/players'
  import { DEFAULT_LIBRARY_PREFS, type BrowserTrackStyle } from '$lib/library.svelte'
  import type { WaveformMode } from '$lib/waveform/data'
  import type { Choice } from './choice'
  import { allOf, setting } from './resettable'
  import SettingChoice from './SettingChoice.svelte'
  import SettingMenu from './SettingMenu.svelte'
  import SettingSlider from './SettingSlider.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { library, decks } = getApp()
  const waveform = decks.prefs.waveform

  const PLAYER_CHOICES: Choice[] = PLAYERS.map((player) => ({ id: player.id, label: player.name }))
  const TRACK_STYLES: Choice<BrowserTrackStyle>[] = [
    { id: 'standard', label: 'Standard' },
    { id: 'alternative', label: 'Alternative' }
  ]

  const defaults = allOf(
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
  )
</script>

<SettingsPage {title} {defaults}>
  <SettingsGroup title="Browser">
    <SettingChoice
      label="Track style"
      description="Standard shows more tracks. Alternative combines the title and artist, but shows the full artwork."
      options={TRACK_STYLES}
      current={library.browserTrackStyle}
      onpick={(style) => library.setBrowserTrackStyle(style)}
    />
    <SettingSlider
      label="Track height"
      description="Height of each track in the browser. Text, artwork and waveforms scale with it."
      min={ROW_HEIGHT_MIN}
      max={ROW_HEIGHT_MAX}
      step={ROW_HEIGHT_STEP}
      value={library.browserRowHeight}
      valueText="{library.browserRowHeight}px"
      onchange={(height) => library.setBrowserRowHeight(height)}
    />
    <SettingToggle
      label="Hide streaming tracks"
      description="Leave tracks from streaming services out of the collection, playlists, histories and tags. They stay in rekordbox."
      on={library.hideStreaming}
      onflip={() => library.setHideStreaming(!library.hideStreaming)}
    />
    <SettingToggle
      label="Hide preset tracks"
      description="Leave the sampler presets rekordbox installs, such as Groove Circuit, out of the collection, playlists, histories and tags. Samples you record stay listed. The presets stay in rekordbox."
      on={library.hidePresetTracks}
      onflip={() => library.setHidePresetTracks(!library.hidePresetTracks)}
    />
  </SettingsGroup>

  <SettingsGroup title="Hardware">
    <SettingMenu
      label="Target"
      description="Green plays on this player and every newer one. Yellow plays here, but not on a newer player. Red will not play here."
      options={PLAYER_CHOICES}
      current={library.hardwareGreen}
      onpick={(id) => library.setHardwareGreen(id)}
    />
  </SettingsGroup>

  <SettingsGroup title="Waveforms">
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
    <SettingSlider
      label="Smoothing"
      description="How gently the 3-band waveform eases down after each peak. Off shows hard steps."
      min={0}
      max={MAX_FALL_BLOCKS_PER_QUALITY}
      step={FALL_BLOCKS_PER_QUALITY_STEP}
      value={waveform.fallBlocksPerQuality}
      valueText={waveform.fallBlocksPerQuality === 0 ? 'Off' : `${waveform.fallBlocksPerQuality.toFixed(2)}×`}
      onchange={(blocks) => waveform.setFallBlocksPerQuality(blocks)}
    />
  </SettingsGroup>
</SettingsPage>

{#snippet qualitySlider(label: string, description: string, mode: WaveformMode)}
  {@const quality = waveform.qualities[mode]}
  <SettingSlider
    {label}
    {description}
    min={0}
    max={MAX_QUALITY[mode]}
    step={1}
    value={quality}
    valueText={quality === 0 ? 'Off' : `${quality}%`}
    onchange={(q) => waveform.setQuality(mode, q)}
  />
{/snippet}
