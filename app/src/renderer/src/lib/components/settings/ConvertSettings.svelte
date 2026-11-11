<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
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
  import type { ConvertFormat, ConvertPreset } from '$lib/types'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'

  let { title }: { title: string } = $props()

  const SELECT_CLASS = 'h-7 rounded-md border border-line bg-bg px-2 text-[12px] text-fg'

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

  const sameAs = (a: ConvertPreset[], b: ConvertPreset[]): boolean =>
    a.map(presetKey).join() === b.map(presetKey).join()

  const defaults = {
    isDefault: () => sameAs(presets, DEFAULT_PRESETS),
    reset: () => setPresets(DEFAULT_PRESETS)
  }

  function setPresets(next: ConvertPreset[]): void {
    presets = next
    savePresets(next)
  }

  function removePreset(preset: ConvertPreset): void {
    setPresets(presets.filter((p) => presetKey(p) !== presetKey(preset)))
  }
</script>

<SettingsPage {title} {defaults}>
  <SettingsGroup title="Formats">
    <p class="mb-2 px-2 text-[11px] leading-snug text-faint">These appear under Convert file type in the track menu.</p>
    {#each presets as preset (presetKey(preset))}
      <div class="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover">
        <span class="min-w-0 flex-1 text-[12.5px]">{presetLabel(preset)}</span>
        <button
          class="grid size-6 place-items-center rounded text-faint hover:bg-raised hover:text-danger"
          aria-label="Remove {presetLabel(preset)}"
          onclick={() => removePreset(preset)}
        >
          <Trash2 class="size-3.5" />
        </button>
      </div>
    {:else}
      <p class="px-2 text-[12px] text-faint">No formats. Add one below.</p>
    {/each}
  </SettingsGroup>

  <SettingsGroup title="Add a format">
    <div class="flex flex-wrap items-center gap-2 px-2 py-1">
      <select class={SELECT_CLASS} aria-label="Format" bind:value={draftFormat}>
        {#each FORMATS as f (f)}
          <option value={f}>{FORMAT_NAMES[f]}</option>
        {/each}
      </select>
      {#if isLossless(draftFormat)}
        <select class={SELECT_CLASS} aria-label="Bit depth" bind:value={draftDepth}>
          {#each BIT_DEPTHS as d (d)}
            <option value={d}>{d}-bit</option>
          {/each}
        </select>
        <select class={SELECT_CLASS} aria-label="Sample rate" bind:value={draftRate}>
          {#each SAMPLE_RATES as r (r)}
            <option value={r}>{r / 1000} kHz</option>
          {/each}
        </select>
      {:else}
        <select class={SELECT_CLASS} aria-label="Bitrate" bind:value={draftBitrate}>
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
  </SettingsGroup>
</SettingsPage>
