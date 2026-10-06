<script lang="ts">
  import { RefreshCw, Search, X, PanelRight, Lock, Volume1, Volume2, VolumeX } from '@lucide/svelte'
  import { getApp } from '$lib/context'

  let {
    inspectorOpen = $bindable(true),
    searchInput = $bindable()
  }: {
    inspectorOpen: boolean
    searchInput?: HTMLInputElement
  } = $props()

  const { library, player } = getApp()
  const silent = $derived(player.muted || player.volume === 0)
  const isMac = window.api.platform === 'darwin'

  const snapshotLabel = $derived(
    library.info
      ? new Date(library.info.snapshotTime * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
      : ''
  )
</script>

<header
  class={[
    'drag flex h-10 shrink-0 items-center gap-3 border-b border-line-soft bg-bg pr-3',
    isMac ? 'pl-[84px]' : 'pl-3 pr-[150px]'
  ]}
>
  <div class="flex items-center gap-2 text-[13px] font-semibold tracking-tight">
    <span class="size-2 rounded-full bg-accent shadow-[0_0_12px] shadow-accent/60"></span>
    ClubReady
  </div>

  <div class="no-drag relative ml-4 w-full max-w-md">
    <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-faint" />
    <input
      bind:this={searchInput}
      bind:value={library.query}
      type="search"
      placeholder="Search {library.sourceTitle}"
      spellcheck="false"
      class="h-7 w-full rounded-md border border-line bg-surface pr-7 pl-8 text-[12.5px] text-fg placeholder:text-faint focus:border-accent/60 focus:outline-none [&::-webkit-search-cancel-button]:hidden"
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
        Library changed — reload
      </button>
    {:else if library.info}
      <button
        class="no-drag flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-faint hover:bg-hover hover:text-muted"
        title="Viewing a read-only snapshot of {library.info.dbPath}"
        onclick={() => library.load(true)}
      >
        <Lock class="size-3" />
        Read-only · {snapshotLabel}
        <RefreshCw class={['size-3', library.status === 'loading' && 'animate-spin']} />
      </button>
    {/if}

    <div class="no-drag flex h-7 items-center rounded-md pr-1.5">
      <button
        class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
        onclick={() => player.toggleMute()}
        aria-label={silent ? 'Unmute' : 'Mute'}
        aria-pressed={silent}
        title={silent ? 'Unmute' : 'Mute'}
      >
        {#if silent}
          <VolumeX class="size-3.5" />
        {:else if player.volume < 0.5}
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
        aria-valuetext="{Math.round(player.volume * 100)}%"
        title="Volume {Math.round(player.volume * 100)}%"
        bind:value={() => player.volume, (v) => player.setVolume(v)}
      />
    </div>

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
