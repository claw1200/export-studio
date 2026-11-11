<script lang="ts">
  import { getApp } from '$lib/context'
  import { HEIGHT_SCALE_STEP, MIN_HEIGHT_SCALE, type WaveformView } from '$lib/deck/display.svelte'
  import { devOptions } from '$lib/devOptions.svelte'
  import { MEMORY_CUE_LIMIT } from '$lib/player.svelte'
  import { BAND_NAMES_LOW_TO_HIGH, BLEND_MODES, bandStyle } from '$lib/waveform/bandStyle.svelte'
  import ConfirmDialog from '../ConfirmDialog.svelte'
  import { allOf } from './resettable'
  import SettingMenu from './SettingMenu.svelte'
  import SettingSlider from './SettingSlider.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { decks } = getApp()
  const prefs = decks.prefs

  const defaults = allOf(
    { isDefault: () => prefs.developerOptionsAreDefault, reset: () => prefs.resetDeveloperOptions() },
    { isDefault: () => bandStyle.layeringIsDefault, reset: () => bandStyle.resetLayering() }
  )

  let confirming = $state<'turnOn' | 'turnOffAndReset' | null>(null)

  function flipDevOptions(): void {
    if (!devOptions.enabled) confirming = 'turnOn'
    else if (!defaults.isDefault()) confirming = 'turnOffAndReset'
    else turnOff()
  }

  function turnOff(): void {
    devOptions.setEnabled(false)
    defaults.reset()
  }
</script>

<SettingsPage {title} {defaults}>
  <SettingToggle
    label="Enable developer options"
    description="Settings for testing and troubleshooting."
    on={devOptions.enabled}
    onflip={flipDevOptions}
  />
  <fieldset class="mt-3 min-w-0 border-t border-line-soft pt-3 disabled:opacity-40" disabled={!devOptions.enabled}>
    <SettingToggle
      label="Snap playhead"
      description="Draw the playhead exactly where the audio engine reports it, instead of easing small corrections. Can look less smooth."
      on={prefs.snapPlayhead}
      onflip={() => prefs.setSnapPlayhead(!prefs.snapPlayhead)}
    />
    <SettingToggle
      label="Unlimited memory cues"
      description="Place more than {MEMORY_CUE_LIMIT} memory cues on a track."
      on={prefs.unlimitedMemoryCues}
      onflip={() => prefs.setUnlimitedMemoryCues(!prefs.unlimitedMemoryCues)}
    />
    <SettingsGroup title="Waveform height">
      {@render heightScaleSlider('Player waveform', 'Height of the scrolling waveform on each deck.', 'detail')}
      {@render heightScaleSlider(
        'Overview waveforms',
        'Height of the whole-track waveform on each deck and in the browser.',
        'overview'
      )}
    </SettingsGroup>
    <SettingsGroup title="Waveform colors">
      {#each BAND_NAMES_LOW_TO_HIGH as name, band (name)}
        {#if band > 0}
          <SettingSlider
            label="{name} opacity"
            description="How much of the bands beneath show through the {name.toLowerCase()} band where they overlap."
            min={0}
            max={1}
            step={0.05}
            value={bandStyle.layeredOpacities[band]}
            valueText="{Math.round(bandStyle.layeredOpacities[band] * 100)}%"
            onchange={(opacity) => bandStyle.setLayeredOpacity(band, opacity)}
          />
        {/if}
      {/each}
      <SettingMenu
        label="Blend"
        description="How each band mixes with the bands beneath it where they overlap."
        options={BLEND_MODES}
        current={bandStyle.blend}
        onpick={(mode) => bandStyle.setBlend(mode)}
      />
    </SettingsGroup>
  </fieldset>
</SettingsPage>

{#if confirming === 'turnOn'}
  <ConfirmDialog
    title="Turn on developer options?"
    message="Developer options are for testing and troubleshooting, and may allow you to cause damage to your library if you are not careful."
    confirmLabel="Turn on"
    danger
    onconfirm={() => devOptions.setEnabled(true)}
    onclose={() => (confirming = null)}
  />
{:else if confirming === 'turnOffAndReset'}
  <ConfirmDialog
    title="Reset developer options?"
    message="Turning off developer options will reset all settings in this category to their defaults."
    confirmLabel="Turn off and reset"
    danger
    onconfirm={turnOff}
    onclose={() => (confirming = null)}
  />
{/if}

{#snippet heightScaleSlider(label: string, description: string, view: WaveformView)}
  {@const scale = prefs.waveform.heightScales[view]}
  <SettingSlider
    {label}
    {description}
    min={MIN_HEIGHT_SCALE}
    max={1}
    step={HEIGHT_SCALE_STEP}
    value={scale}
    valueText="{Math.round(scale * 100)}%"
    onchange={(s) => prefs.waveform.setHeightScale(view, s)}
  />
{/snippet}
