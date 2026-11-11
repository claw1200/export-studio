<script lang="ts">
  import { accent, DEFAULT_ACCENT } from '$lib/accent.svelte'
  import { getApp } from '$lib/context'
  import { DEFAULT_CUSTOM_RGB_COLORS } from '$lib/deck/display.svelte'
  import { BAND_NAMES_LOW_TO_HIGH, bandStyle, DEFAULT_BAND_STYLE } from '$lib/waveform/bandStyle.svelte'
  import { DEFAULT_RGB_GRADIENT, rgbGradient } from '$lib/waveform/rgbGradient.svelte'
  import { allOf, setting } from './resettable'
  import SettingColor from './SettingColor.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { decks } = getApp()
  const waveform = decks.prefs.waveform

  const defaults = allOf(
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
  )
</script>

<SettingsPage {title} {defaults}>
  <SettingColor
    label="Accent color"
    description="Buttons, highlights, and progress."
    color={accent.color}
    defaultColor={DEFAULT_ACCENT}
    onchange={(color) => accent.set(color)}
  />
  <SettingsGroup title="Color waveform">
    <SettingToggle
      label="Custom colors"
      description="Color the waveform by sound, from low to high. Off uses the rekordbox colors."
      on={waveform.customRgbColors}
      onflip={() => waveform.setCustomRgbColors(!waveform.customRgbColors)}
    />
    {#if waveform.customRgbColors}
      {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
        <SettingColor
          label="{name} color"
          description="The color for the {name.toLowerCase()} band."
          color={rgbGradient.stopsLowToHigh[band]}
          defaultColor={DEFAULT_RGB_GRADIENT[band]}
          onchange={(color) => rgbGradient.setStop(band, color)}
        />
      {/each}
    {/if}
  </SettingsGroup>
  <SettingsGroup title="3-band waveform">
    {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
      <SettingColor
        label="{name} color"
        description="Fill color for the {name.toLowerCase()} band."
        color={bandStyle.colors[band]}
        defaultColor={DEFAULT_BAND_STYLE.colors[band]}
        onchange={(color) => bandStyle.setColor(band, color)}
      />
    {/each}
  </SettingsGroup>
</SettingsPage>
