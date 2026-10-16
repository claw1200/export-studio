<script lang="ts">
  import { Folder, FolderOpen, ListMusic, LoaderCircle, X } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import type { PlaylistPolicy, UsbImportPreview, UsbImportResult } from '$lib/types'

  const POLICIES: { value: PlaylistPolicy; label: string; hint: string }[] = [
    { value: 'skipExisting', label: 'Leave existing playlists alone', hint: 'Playlists the library already has keep their tracks.' },
    { value: 'merge', label: 'Add to existing playlists', hint: 'Tracks from the device are added to playlists with the same name.' },
    { value: 'createNew', label: 'Always create new playlists', hint: 'Every playlist on the device becomes a new one.' }
  ]

  let { volumeId, onclose }: { volumeId: string; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  const volume = $derived(library.devices.find((item) => item.id === volumeId) ?? null)

  let folder = $state<string | null>(null)
  let policy = $state<PlaylistPolicy>('skipExisting')
  let preview = $state.raw<UsbImportPreview | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)
  let result = $state<UsbImportResult | null>(null)
  let token = 0

  const locked = $derived(library.syncing)
  const counts = $derived(preview?.counts ?? { reuse: 0, copy: 0, missing: 0 })
  const importable = $derived(!!preview && preview.tracks.length > counts.missing)

  async function load(): Promise<void> {
    const current = ++token
    loading = true
    error = null
    try {
      const next = await api.usbImportPreview(volumeId, folder, policy)
      if (current === token) preview = next
    } catch (reason) {
      if (current !== token) return
      preview = null
      error = reason instanceof Error ? reason.message : 'Couldn’t read this device'
    } finally {
      if (current === token) loading = false
    }
  }

  onMount(() => void load())

  async function chooseFolder(): Promise<void> {
    const picked = await api.choosePath('folder', folder ?? preview?.destination)
    if (!picked) return
    folder = picked
    void load()
  }

  function resetFolder(): void {
    folder = null
    void load()
  }

  async function submit(): Promise<void> {
    if (locked || loading || !importable) return
    error = null
    const outcome = await library.importDevice(volumeId, folder, policy)
    if (!outcome) {
      error = library.syncError
      return
    }
    if (!outcome.tracks && outcome.failed.length) {
      error = outcome.failed[0].error
      return
    }
    result = outcome
  }

  function progressLabel(): string {
    const progress = library.syncProgress
    if (progress?.phase !== 'import') return 'Importing'
    const total = progress.total ?? 0
    if (!total) return 'Importing'
    const name = progress.name ? ` · ${progress.name}` : ''
    return `Importing ${Math.min((progress.done ?? 0) + 1, total)} of ${total}${name}`
  }

  function plural(n: number, word: string): string {
    return `${n.toLocaleString()} ${word}${n === 1 ? '' : 's'}`
  }

  const STATUS_LABEL = { reuse: 'In library', copy: 'New', missing: 'File missing' } as const
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto w-[720px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Import from device"
  onclose={() => onclose()}
  onclick={(e) => {
    if (locked) return
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
  oncancel={(e) => {
    if (locked) e.preventDefault()
  }}
>
  <form
    method="dialog"
    class="flex max-h-[calc(100vh-48px)] flex-col"
    onsubmit={(e) => {
      if (e.submitter?.getAttribute('value') !== 'confirm') return
      e.preventDefault()
      void submit()
    }}
  >
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
      <h2 class="min-w-0 flex-1 truncate text-[13px] font-semibold">Import from {volume?.name ?? 'device'}</h2>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
        aria-label="Close"
        disabled={locked}
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      {#if result}
        <p class="text-[12.5px] text-ok">
          Imported {plural(result.playlists, 'playlist')} and {plural(result.tracks, 'track')}
          ({result.copied} copied, {result.reused} already in the library).
        </p>
        {#if result.failed.length}
          <p class="mt-3 text-[12px] font-semibold text-warn">{plural(result.failed.length, 'track')} couldn’t be imported</p>
          <div class="mt-1.5 max-h-48 overflow-y-auto rounded-md border border-line">
            {#each result.failed as item, i (i)}
              <p class="truncate px-2 py-1.5 text-[12px]" title={item.error}>
                {item.title}<span class="text-faint"> · {item.error}</span>
              </p>
            {/each}
          </div>
        {/if}
      {:else}
        <p class="text-[12.5px] leading-relaxed text-muted">
          Review what will be added to the library. Tracks already in the library are reused, and nothing on the
          USB is changed.
        </p>

        <p class="mt-4 text-[11px] font-semibold tracking-wider text-faint uppercase">Copy new tracks to</p>
        <div class="mt-1.5 flex items-center gap-2">
          <span
            class="flex h-8 min-w-0 flex-1 items-center truncate rounded-md border border-line bg-bg px-2 text-[12.5px]"
            title={preview?.destination}
          >
            {preview?.destination ?? '…'}
          </span>
          <button
            type="button"
            disabled={locked}
            class="flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-raised px-3 text-[12.5px] hover:bg-hover disabled:opacity-40"
            onclick={chooseFolder}
          >
            <FolderOpen class="size-3.5" />
            Choose…
          </button>
          {#if folder}
            <button
              type="button"
              disabled={locked}
              class="h-8 shrink-0 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
              onclick={resetFolder}
            >
              Use default
            </button>
          {/if}
        </div>
        {#if preview}
          <p class="mt-1.5 text-[11px] text-faint">
            {folder ? 'Files go in' : 'Default rekordbox location. Files go in'}
            {preview.layout}/ folders.
          </p>
        {/if}

        <label class="mt-4 flex items-center gap-3 text-[12.5px] text-muted">
          <span class="shrink-0">Playlists</span>
          <select
            bind:value={policy}
            disabled={locked}
            class="h-8 min-w-0 flex-1 rounded-md bg-raised px-2 text-[12.5px] text-fg disabled:opacity-40"
            onchange={() => void load()}
          >
            {#each POLICIES as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>
        <p class="mt-1.5 text-[11px] text-faint">
          {POLICIES.find((option) => option.value === policy)?.hint}
        </p>

        {#if loading}
          <p class="mt-4 flex items-center gap-2 text-[12.5px] text-muted">
            <LoaderCircle class="size-3.5 animate-spin" />
            Reading this device…
          </p>
        {:else if preview}
          <p class="mt-4 text-[12.5px] text-muted">
            {plural(preview.playlists.filter((p) => !p.folder).length, 'playlist')} ·
            {counts.copy} new to copy · {counts.reuse} already in the library
            {#if counts.missing}· <span class="text-warn">{counts.missing} with missing audio</span>{/if}
          </p>

          <div class="mt-4 grid grid-cols-2 items-start gap-4">
            <section class="min-w-0">
              <h3 class="text-[11px] font-semibold tracking-wider text-faint uppercase">Playlists</h3>
              <div class="mt-1.5 max-h-72 overflow-y-auto rounded-md border border-line py-1">
                {#each preview.playlists as item, i (i)}
                  <div
                    class="flex h-7 items-center gap-1.5 pr-2 text-[12.5px] text-muted"
                    style:padding-left="{8 + item.depth * 14}px"
                  >
                    {#if item.folder}
                      <Folder class="size-3.5 shrink-0 text-faint" />
                    {:else}
                      <ListMusic class="size-3.5 shrink-0 text-faint" />
                    {/if}
                    <span class="min-w-0 flex-1 truncate">{item.name}</span>
                    {#if !item.folder}
                      <span class="tabular text-[11px] text-faint">{item.tracks}</span>
                    {/if}
                    <span class={['text-[11px]', item.exists ? 'text-faint' : 'text-ok']}>
                      {item.exists ? 'Exists' : 'New'}
                    </span>
                  </div>
                {:else}
                  <p class="px-2 py-1 text-[12px] text-faint">No playlists on this device</p>
                {/each}
              </div>
            </section>
            <section class="min-w-0">
              <h3 class="text-[11px] font-semibold tracking-wider text-faint uppercase">Tracks</h3>
              <div class="mt-1.5 max-h-72 overflow-y-auto rounded-md border border-line py-1">
                {#each preview.tracks as item, i (i)}
                  <div class="flex h-7 items-center gap-1.5 px-2 text-[12.5px] text-muted">
                    <span class="min-w-0 flex-1 truncate" title="{item.artist} – {item.title}">{item.title}</span>
                    <span
                      class={[
                        'shrink-0 text-[11px]',
                        item.status === 'copy' ? 'text-ok' : item.status === 'missing' ? 'text-warn' : 'text-faint'
                      ]}
                    >
                      {STATUS_LABEL[item.status]}
                    </span>
                  </div>
                {:else}
                  <p class="px-2 py-1 text-[12px] text-faint">No tracks</p>
                {/each}
              </div>
            </section>
          </div>
        {/if}

        {#if !library.writable}
          <p class="mt-3 text-[12px] text-warn">Unlock editing to import tracks.</p>
        {/if}
        {#if error}
          <p class="mt-3 text-[12px] leading-relaxed text-warn">{error}</p>
        {/if}
        {#if locked}
          <p class="mt-3 text-[11px] text-muted">{progressLabel()}</p>
        {/if}
      {/if}
    </div>

    <footer class="flex shrink-0 justify-end gap-2 border-t border-line-soft px-5 py-3">
      <button
        type="button"
        disabled={locked}
        class="h-8 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      >
        {result ? 'Done' : 'Cancel'}
      </button>
      {#if !result}
        <button
          value="confirm"
          disabled={locked || loading || !importable || !library.writable}
          class="h-8 rounded-md bg-accent px-4 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
        >
          Import
        </button>
      {/if}
    </footer>
  </form>
</dialog>
