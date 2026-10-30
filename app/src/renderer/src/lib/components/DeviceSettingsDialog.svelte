<script lang="ts">
  import { ArrowDown, ArrowUp, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import type { DeviceMenuSlot, DeviceSettings } from '../../../../preload/engine'

  let { volumeId, onclose }: { volumeId: string; onclose: () => void } = $props()
  const { library, keybinds } = getApp()
  const volume = $derived(library.devices.find((item) => item.id === volumeId))
  let settings = $state<DeviceSettings | null>(null)
  let initial = $state('')
  let loading = $state(true)
  let saving = $state(false)
  let error = $state('')
  let tab = $state<'display' | 'browse' | 'colors'>('display')
  const dirty = $derived(settings !== null && JSON.stringify(settings) !== initial)
  const locked = $derived(loading || saving || library.syncing || !volume || !!library.info?.rekordboxRunning)
  const colors = [
    { name: 'Default', value: 'var(--color-muted)' },
    { name: 'Pink', value: '#ec83b6' },
    { name: 'Red', value: '#e65a5a' },
    { name: 'Orange', value: '#ee9b4a' },
    { name: 'Yellow', value: '#e5cc50' },
    { name: 'Green', value: '#63ba77' },
    { name: 'Aqua', value: '#59c8c0' },
    { name: 'Blue', value: '#609de4' },
    { name: 'Purple', value: '#ac83d8' }
  ]
  const inputClass = 'h-8 min-w-0 w-full rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg'

  async function load(): Promise<void> {
    if (saving || library.syncing || !volume) return
    loading = true
    error = ''
    settings = null
    try {
      settings = await api.usbSettings(volumeId)
      initial = JSON.stringify(settings)
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'Could not read device settings'
    } finally {
      loading = false
    }
  }

  onMount(() => void load())

  async function save(): Promise<void> {
    if (locked || !settings || !dirty) return
    saving = true
    error = ''
    try {
      settings = await api.usbSetSettings(volumeId, $state.snapshot(settings))
      initial = JSON.stringify(settings)
      library.notify('Device settings saved')
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'Could not save device settings'
    } finally {
      saving = false
    }
  }

  function ordered(slots: DeviceMenuSlot[]): DeviceMenuSlot[] {
    return [...slots].sort(
      (first, second) =>
        Number(second.visible) - Number(first.visible) ||
        (first.sequence || 1000) - (second.sequence || 1000) ||
        first.id - second.id
    )
  }

  function toggle(slot: DeviceMenuSlot, slots: DeviceMenuSlot[]): void {
    slot.visible = !slot.visible
    slot.sequence = slot.visible ? Math.max(0, ...slots.map((item) => item.sequence)) + 1 : 0
  }

  function move(slot: DeviceMenuSlot, slots: DeviceMenuSlot[], direction: number): void {
    const active = ordered(slots).filter((item) => item.visible)
    const index = active.findIndex((item) => item.id === slot.id)
    const next = index + direction
    if (index < 0 || next < 0 || next >= active.length) return
    ;[active[index], active[next]] = [active[next], active[index]]
    active.forEach((item, position) => {
      item.sequence = position + 1
    })
  }
</script>

{#snippet background(label: string, value: number, change: (value: number) => void)}
  <fieldset class="min-w-0">
    <legend class="mb-2 text-[12px] text-muted">{label}</legend>
    <div class="flex flex-wrap gap-2">
      {#each colors as color, index (color.name)}
        <button
          type="button"
          class={[
            'grid size-7 shrink-0 place-items-center rounded-md border',
            value === index ? 'border-accent bg-select' : 'border-transparent hover:bg-hover'
          ]}
          title={color.name}
          aria-label={`${label}: ${color.name}`}
          aria-pressed={value === index}
          onclick={() => change(index)}
        >
          <span class="size-4 rounded-sm border border-white/15" style:background={color.value}></span>
        </button>
      {/each}
    </div>
  </fieldset>
{/snippet}

{#snippet menuList(label: string, slots: DeviceMenuSlot[])}
  <section class="min-w-0">
    <h3 class="mb-2 text-[12px] font-semibold">{label}</h3>
    <div class="max-h-64 overflow-y-auto border-y border-line-soft">
      {#each ordered(slots) as slot (slot.id)}
        {@const active = ordered(slots).filter((item) => item.visible)}
        <div class="flex min-h-9 items-center gap-2 border-b border-line-soft last:border-b-0">
          <label class="flex min-w-0 flex-1 items-center gap-2 py-1 text-[12px]">
            <input type="checkbox" checked={slot.visible} onchange={() => toggle(slot, slots)} />
            <span class="truncate" title={slot.name}>{slot.name}</span>
          </label>
          <button
            type="button"
            class="grid size-7 shrink-0 place-items-center rounded-md text-muted hover:bg-hover disabled:opacity-20"
            title={`Move ${slot.name} up`}
            aria-label={`Move ${slot.name} up`}
            disabled={!slot.visible || active[0]?.id === slot.id}
            onclick={() => move(slot, slots, -1)}
          >
            <ArrowUp class="size-3.5" />
          </button>
          <button
            type="button"
            class="grid size-7 shrink-0 place-items-center rounded-md text-muted hover:bg-hover disabled:opacity-20"
            title={`Move ${slot.name} down`}
            aria-label={`Move ${slot.name} down`}
            disabled={!slot.visible || active.at(-1)?.id === slot.id}
            onclick={() => move(slot, slots, 1)}
          >
            <ArrowDown class="size-3.5" />
          </button>
        </div>
      {/each}
    </div>
  </section>
{/snippet}

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto w-[660px] max-w-[calc(100vw-32px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Device settings"
  {onclose}
  oncancel={(event) => {
    if (saving) event.preventDefault()
  }}
  onclick={(event) => {
    if (!saving && event.target === event.currentTarget) event.currentTarget.close()
  }}
>
  <form
    class="flex max-h-[calc(100dvh-32px)] flex-col"
    onsubmit={(event) => {
      event.preventDefault()
      void save()
    }}
  >
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-line-soft px-5">
      <h2 class="min-w-0 flex-1 truncate text-[13px] font-semibold">
        Device settings{volume ? `: ${volume.name}` : ''}
      </h2>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-muted hover:bg-hover disabled:opacity-40"
        aria-label="Reload settings"
        title="Reload settings"
        disabled={loading || saving || library.syncing || !volume}
        onclick={() => void load()}
      >
        <RefreshCw class="size-3.5" />
      </button>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-muted hover:bg-hover disabled:opacity-40"
        aria-label="Close"
        title="Close"
        disabled={saving}
        onclick={(event) => event.currentTarget.closest('dialog')?.close()}
      >
        <X class="size-4" />
      </button>
    </header>
    <nav class="flex shrink-0 gap-4 border-b border-line-soft px-5" aria-label="Settings sections">
      {#each ['display', 'browse', 'colors'] as item (item)}
        <button
          type="button"
          class={[
            'h-10 border-b-2 text-[12.5px] capitalize',
            tab === item ? 'border-accent text-fg' : 'border-transparent text-muted hover:text-fg'
          ]}
          aria-current={tab === item ? 'page' : undefined}
          onclick={() => (tab = item as typeof tab)}>{item}</button
        >
      {/each}
    </nav>
    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      {#if !volume}
        <p role="status" class="text-[12px] text-warn">Device disconnected</p>
      {:else if library.info?.rekordboxRunning}
        <p role="status" class="mb-4 text-[12px] text-warn">Close rekordbox before changing device settings.</p>
      {:else if library.syncing}
        <p role="status" class="mb-4 text-[12px] text-warn">A device operation is in progress.</p>
      {/if}
      {#if loading}
        <div role="status" class="flex h-40 items-center justify-center gap-2 text-[12px] text-muted">
          <LoaderCircle class="size-4 animate-spin" />Loading settings
        </div>
      {:else if settings}
        <fieldset disabled={locked} class="min-w-0 space-y-5 disabled:opacity-50">
          {#if tab === 'display'}
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Waveform color
                <select class={inputClass} bind:value={settings.display.color}
                  ><option value="blue">Blue</option><option value="rgb">RGB</option><option value="3band">3Band</option
                  ></select
                >
              </label>
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Overview waveform
                <select class={inputClass} bind:value={settings.display.overview}
                  ><option value="half">Half</option><option value="full">Full</option></select
                >
              </label>
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Key display format
                <select class={inputClass} bind:value={settings.display.keyDisplay}
                  ><option value="classic">Classic</option><option value="alphanumeric">Alphanumeric</option></select
                >
              </label>
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Waveform current position
                <select class={inputClass} bind:value={settings.display.position}
                  ><option value="center">Center</option><option value="left">Left</option></select
                >
              </label>
            </div>
          {:else if tab === 'browse'}
            {#if settings.library}
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Device name
                <input class={inputClass} bind:value={settings.library.deviceName} maxlength="63" />
              </label>
              <label class="grid gap-1.5 text-[12px] text-muted"
                >Sub-column
                <select class={inputClass} bind:value={settings.library.subColumn}>
                  <option value={null}>Not specified</option>
                  {#each settings.library.sorts as slot (slot.id)}<option value={slot.menuItem}>{slot.name}</option
                    >{/each}
                </select>
              </label>
              <div class="grid grid-cols-1 gap-5 sm:grid-cols-2">
                {@render menuList('Categories', settings.library.categories)}
                {@render menuList('Sort options', settings.library.sorts)}
              </div>
            {:else}
              <p class="py-8 text-[12px] text-muted">Browse settings unavailable: no OneLibrary database.</p>
            {/if}
          {:else}
            {#if settings.deviceLibraryBackground !== null}
              {@render background('Device Library background', settings.deviceLibraryBackground, (value) => {
                if (settings) settings.deviceLibraryBackground = value
              })}
            {/if}
            {#if settings.library}
              {@render background('OneLibrary background', settings.library.backgroundColor, (value) => {
                if (settings?.library) settings.library.backgroundColor = value
              })}
              <section>
                <h3 class="mb-3 text-[12px] font-semibold">Color labels</h3>
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  {#each settings.library.colors as color (color.id)}
                    <label class="flex min-w-0 items-center gap-2">
                      <span
                        class="size-4 shrink-0 rounded-sm"
                        aria-hidden="true"
                        style:background={colors[color.id]?.value}
                        title={colors[color.id]?.name}
                      ></span>
                      <span class="sr-only">{colors[color.id]?.name ?? `Color ${color.id}`} label</span>
                      <input class={inputClass} bind:value={color.name} maxlength="63" />
                    </label>
                  {/each}
                </div>
              </section>
            {:else}
              <p class="text-[12px] text-muted">Color labels unavailable: no OneLibrary database.</p>
            {/if}
          {/if}
        </fieldset>
      {/if}
      {#if error}<p role="alert" class="mt-4 break-words text-[12px] text-danger">{error}</p>{/if}
    </div>
    <footer class="flex shrink-0 items-center justify-end gap-2 border-t border-line-soft px-5 py-3">
      <button
        type="button"
        class="h-8 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover disabled:opacity-40"
        disabled={saving}
        onclick={(event) => event.currentTarget.closest('dialog')?.close()}>{dirty ? 'Cancel' : 'Close'}</button
      >
      <button
        type="submit"
        class="flex h-8 items-center gap-2 rounded-md bg-accent px-4 text-[12.5px] font-medium text-white disabled:opacity-40"
        disabled={locked || !dirty}
      >
        {#if saving}<LoaderCircle class="size-3.5 animate-spin" />{/if}{saving ? 'Saving' : 'Apply'}
      </button>
    </footer>
  </form>
</dialog>
