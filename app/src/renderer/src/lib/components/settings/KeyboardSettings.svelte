<script lang="ts">
  import { Search } from '@lucide/svelte'
  import type { Attachment } from 'svelte/attachments'
  import { on } from 'svelte/events'
  import { getApp } from '$lib/context'
  import {
    ACTION_IDS,
    ACTIONS,
    chordFromEvent,
    chordLabel,
    LAYER_MODIFIERS,
    layerModifierName,
    type ActionGroup,
    type ActionId
  } from '$lib/keymap'
  import type { Choice } from './choice'
  import SettingMenu from './SettingMenu.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import ShortcutRow from './ShortcutRow.svelte'

  let { title }: { title: string } = $props()

  const { keybinds } = getApp()

  const GROUPS: ActionGroup[] = ['Playback', 'Cues', 'Beatgrid', 'Library', 'App']
  const MODIFIER_CHOICES = LAYER_MODIFIERS.map((modifier) => ({
    id: modifier,
    label: layerModifierName(modifier)
  })) satisfies Choice[]

  let recording = $state<ActionId | null>(null)
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

  const defaults = { isDefault: () => !keybinds.customised, reset: resetKeybinds }

  function toggleRecording(id: ActionId): void {
    notice = null
    recording = recording === id ? null : id
  }

  function recordChord(e: KeyboardEvent): void {
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

  function swallowRecordedKeyup(e: KeyboardEvent): void {
    if (swallowKeyup !== e.code) return
    swallowKeyup = null
    e.preventDefault()
  }

  function stayOpenWhileRecording(e: Event): void {
    if (!recording) return
    e.preventDefault()
    recording = null
  }

  const captureRecordingKeys: Attachment<HTMLElement> = (node) => {
    const dialog = node.closest('dialog')!
    const stops = [
      on(dialog, 'keydown', recordChord),
      on(dialog, 'keyup', swallowRecordedKeyup),
      on(dialog, 'cancel', stayOpenWhileRecording)
    ]
    return () => stops.forEach((stop) => stop())
  }

  function resetKeybinds(): void {
    recording = null
    notice = null
    keybinds.resetAll()
  }
</script>

<SettingsPage {title} {defaults}>
  {#snippet headerTools()}
    <div class="relative w-56">
      <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-faint" />
      <input
        bind:value={filter}
        type="search"
        placeholder="Filter shortcuts"
        spellcheck="false"
        class="h-7 w-full rounded-md border border-line bg-bg pr-2 pl-7 text-[12px] text-fg placeholder:text-faint focus:border-accent/60 [&::-webkit-search-cancel-button]:hidden"
      />
    </div>
  {/snippet}

  {#snippet footerHint()}
    {#if recording}
      <span class="text-accent">Press a key combination. Esc cancels.</span>
    {:else if notice}
      <span class="truncate text-warn" title={notice}>{notice}</span>
    {:else}
      <span class="text-faint">Click + to add a shortcut. Hover a shortcut to remove it.</span>
    {/if}
  {/snippet}

  <div {@attach captureRecordingKeys}>
    <SettingMenu
      label="Layer modifier"
      description="In mix mode the deck shortcuts control deck A. Hold this key with them to control deck B. It also swaps what the BPM and key steppers show."
      options={MODIFIER_CHOICES}
      current={keybinds.layerModifier}
      onpick={(modifier) => keybinds.setLayerModifier(modifier)}
    />
    {#each groups as { group, ids } (group)}
      <SettingsGroup title={group}>
        {#each ids as id (id)}
          <ShortcutRow {id} recording={recording === id} ontogglerecording={() => toggleRecording(id)} />
        {/each}
      </SettingsGroup>
    {:else}
      <p class="mt-5 px-2 text-[12px] text-faint">No shortcuts match “{filter}”.</p>
    {/each}
  </div>
</SettingsPage>
