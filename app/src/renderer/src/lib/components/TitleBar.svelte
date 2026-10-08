<script lang="ts">
  import { RefreshCw, Search, X, PanelRight, Lock, LockOpen, Settings, Volume1, Volume2, VolumeX } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import SettingsDialog from './SettingsDialog.svelte'

  let {
    inspectorOpen = $bindable(true),
    searchInput = $bindable()
  }: {
    inspectorOpen: boolean
    searchInput?: HTMLInputElement
  } = $props()

  const { library, player, keybinds } = getApp()
  const silent = $derived(player.output.muted || player.output.volume === 0)
  const isMac = window.api.platform === 'darwin'
  let settingsOpen = $state(false)

  $effect(() => keybinds.register({ 'app.settings': () => (settingsOpen = true) }))

  const snapshotLabel = $derived(
    library.info?.snapshotTime
      ? new Date(library.info.snapshotTime * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
      : ''
  )
  const writeBlocked = $derived(!library.writable && !!library.info?.rekordboxRunning)
  const lockTitle = $derived(
    library.lockError
      ? library.lockError
      : library.writable
        ? 'Editing the library. Lock to return to a read-only snapshot.'
        : writeBlocked
          ? 'Close rekordbox to edit the library'
          : 'Unlock to edit the library'
  )

  function toggleLock(): void {
    const on = !library.writable
    void library.setWritable(on).then((changed) => {
      if (changed) void player.refreshDetail()
    })
  }
</script>

{#if settingsOpen}
  <SettingsDialog onclose={() => (settingsOpen = false)} />
{/if}

<header
  class={[
    'drag relative z-20 flex h-10 shrink-0 items-center gap-3 border-b border-line-soft bg-bg pr-3',
    isMac ? 'pl-[84px]' : 'pl-3 pr-[150px]'
  ]}
>
  <div class="no-drag relative ml-4 w-full max-w-md">
    <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-faint" />
    <input
      bind:this={searchInput}
      bind:value={library.query}
      type="search"
      placeholder="Search {library.sourceTitle}"
      spellcheck="false"
      class="h-7 w-full rounded-md border border-line bg-surface pr-7 pl-8 text-[12.5px] text-fg placeholder:text-faint focus:border-accent/60 [&::-webkit-search-cancel-button]:hidden"
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          library.query = ''
          e.currentTarget.blur()
        }
      }}
    />
    {#if library.query}
      <button
        class="absolute top-1/2 right-1.5 grid size-5 -translate-y-1/2 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
        onclick={() => (library.query = '')}
        aria-label="Clear search"
      >
        <X class="size-3" />
      </button>
    {/if}
  </div>

  <div class="ml-auto flex items-center gap-2">
    {#if library.changed}
      <button
        class="no-drag flex h-7 items-center gap-1.5 rounded-md bg-warn/15 px-2.5 text-[12px] font-medium text-warn hover:bg-warn/25"
        onclick={() => library.load(true)}
      >
        <RefreshCw class="size-3.5" />
        Refresh library
      </button>
    {:else if library.info && !library.writable}
      <button
        class="no-drag flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-faint hover:bg-hover hover:text-muted"
        title="Reload the read-only snapshot of {library.info.dbPath}"
        onclick={() => library.load(true)}
      >
        {snapshotLabel}
        <RefreshCw class={['size-3', library.status === 'loading' && 'animate-spin']} />
      </button>
    {/if}

    {#if library.info}
      <button
        class={[
          'no-drag flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] font-medium disabled:cursor-not-allowed disabled:opacity-50',
          library.writable
            ? 'bg-accent-soft text-accent hover:bg-accent/25'
            : 'text-faint hover:bg-hover hover:text-muted disabled:hover:bg-transparent disabled:hover:text-faint'
        ]}
        onclick={toggleLock}
        disabled={writeBlocked || library.switching}
        aria-pressed={library.writable}
        title={lockTitle}
      >
        {#if library.switching}
          <RefreshCw class="size-3 animate-spin" />
        {:else if library.writable}
          <LockOpen class="size-3" />
        {:else}
          <Lock class="size-3" />
        {/if}
        {library.writable ? 'Editing' : 'Read-only'}
      </button>
    {/if}

    <div class="no-drag flex h-7 items-center rounded-md pr-1.5">
      <button
        class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
        onclick={() => player.output.toggleMute()}
        aria-label={silent ? 'Unmute' : 'Mute'}
        aria-pressed={silent}
        title={silent ? 'Unmute' : 'Mute'}
      >
        {#if silent}
          <VolumeX class="size-3.5" />
        {:else if player.output.volume < 0.5}
          <Volume1 class="size-3.5" />
        {:else}
          <Volume2 class="size-3.5" />
        {/if}
      </button>
      <input
        class="h-4 w-16 cursor-pointer accent-accent"
        type="range"
        min="0"
        max="1"
        step="0.01"
        aria-label="Volume"
        aria-valuetext="{Math.round(player.output.volume * 100)}%"
        title="Volume {Math.round(player.output.volume * 100)}%"
        bind:value={() => player.output.volume, (v) => player.output.setVolume(v)}
      />
    </div>

    <button
      class={[
        'no-drag grid size-7 place-items-center rounded-md hover:bg-hover',
        settingsOpen ? 'text-fg' : 'text-faint'
      ]}
      onclick={() => (settingsOpen = true)}
      aria-label="Settings"
      aria-expanded={settingsOpen}
      aria-haspopup="dialog"
      title="Settings"
    >
      <Settings class="size-4" />
    </button>

    <button
      class={[
        'no-drag grid size-7 place-items-center rounded-md hover:bg-hover',
        inspectorOpen ? 'text-fg' : 'text-faint'
      ]}
      onclick={() => (inspectorOpen = !inspectorOpen)}
      aria-label="Toggle inspector"
      aria-pressed={inspectorOpen}
    >
      <PanelRight class="size-4" />
    </button>
  </div>
</header>
