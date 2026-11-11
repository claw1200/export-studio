<script lang="ts">
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import { ABOUT, SECTIONS, type SectionInfo, type SettingsSection } from './sections'

  let { initialSection = 'general', onclose }: { initialSection?: SettingsSection; onclose: () => void } = $props()

  const { keybinds, backups } = getApp()

  // svelte-ignore state_referenced_locally
  let section = $state<SettingsSection>(initialSection)
  const current = $derived([...SECTIONS, ABOUT].find((s) => s.id === section) ?? SECTIONS[0])

  void backups.refresh()
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto h-[600px] max-h-[calc(100vh-48px)] w-[780px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Settings"
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

    <current.page title={current.label} />
  </div>
</dialog>

{#snippet navItem(s: SectionInfo)}
  <button
    class={[
      'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px]',
      section === s.id ? 'bg-raised text-fg' : 'text-muted hover:bg-hover hover:text-fg'
    ]}
    aria-current={section === s.id ? 'page' : undefined}
    onclick={() => (section = s.id)}
  >
    <s.icon class="size-3.5" />
    {s.label}
  </button>
{/snippet}
