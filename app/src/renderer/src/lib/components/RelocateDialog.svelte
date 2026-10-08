<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { FolderOpen, LoaderCircle } from '@lucide/svelte'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import type { MissingScan, Track } from '$lib/types'

  let { tracks, onclose }: { tracks: Track[]; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  let everything = $state(false)
  let folder = $state<string | null>(null)
  let scan = $state.raw<MissingScan | null>(null)
  let scanning = $state(false)
  let applying = $state(false)
  let error = $state<string | null>(null)
  const skipped = new SvelteSet<string>()
  let token = 0

  const extra = $derived(library.missingTracks.length > tracks.length)
  const targets = $derived(everything ? library.missingTracks : tracks)
  const found = $derived(scan?.matches.filter((m) => m.path) ?? [])
  const chosen = $derived(found.filter((m) => !skipped.has(m.id)))
  const busy = $derived(scanning || applying)

  function modal(dialog: HTMLDialogElement): () => void {
    const resume = keybinds.suspend()
    dialog.showModal()
    return resume
  }

  async function run(): Promise<void> {
    if (!folder) return
    const current = ++token
    scanning = true
    error = null
    scan = null
    skipped.clear()
    try {
      const result = await api.findMissing(
        targets.map((t) => t.id),
        folder
      )
      if (current === token) scan = result
    } catch (e) {
      if (current === token) error = (e as Error).message
    } finally {
      if (current === token) scanning = false
    }
  }

  async function chooseFolder(): Promise<void> {
    const picked = await api.choosePath('folder', folder ?? undefined)
    if (!picked) return
    folder = picked
    await run()
  }

  function setEverything(on: boolean): void {
    everything = on
    void run()
  }

  function toggle(id: string): void {
    if (skipped.has(id)) skipped.delete(id)
    else skipped.add(id)
  }

  async function submit(dialog: HTMLDialogElement): Promise<void> {
    if (busy || !chosen.length) return
    applying = true
    error = null
    const ok = await library.relocate(Object.fromEntries(chosen.map((m) => [m.id, m.path as string])))
    applying = false
    if (ok) dialog.close()
    else error = library.editError ?? 'Could not relocate the tracks'
  }

  function plural(n: number, word: string): string {
    return `${n.toLocaleString()} ${word}${n === 1 ? '' : 's'}`
  }
</script>

<dialog
  {@attach modal}
  class="no-drag m-auto w-[520px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Auto-relocate missing tracks"
  onclose={() => onclose()}
  onclick={(e) => {
    if (applying) return
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <form
    method="dialog"
    class="p-5"
    onsubmit={(e) => {
      if (e.submitter?.getAttribute('value') !== 'confirm') return
      e.preventDefault()
      const dialog = e.currentTarget.closest('dialog')
      if (dialog) void submit(dialog)
    }}
  >
    <h2 class="text-[13.5px] font-semibold">Auto-relocate missing tracks</h2>
    <p class="mt-1.5 text-[12.5px] leading-relaxed text-muted">
      Pick a folder to search. Files with the same name as a missing track are matched, including in subfolders.
    </p>

    {#if extra}
      <div class="mt-4 flex gap-4 text-[12.5px]">
        <label class="flex items-center gap-2">
          <input
            type="radio"
            name="relocate-scope"
            checked={!everything}
            disabled={busy}
            onchange={() => setEverything(false)}
          />
          Selected ({tracks.length})
        </label>
        <label class="flex items-center gap-2">
          <input
            type="radio"
            name="relocate-scope"
            checked={everything}
            disabled={busy}
            onchange={() => setEverything(true)}
          />
          All missing tracks ({library.missingTracks.length})
        </label>
      </div>
    {/if}

    <p class="mt-4 text-[11px] font-semibold tracking-wider text-faint uppercase">Search in</p>
    <div class="mt-1.5 flex items-center gap-2">
      <span
        class={[
          'flex h-8 min-w-0 flex-1 items-center truncate rounded-md border border-line bg-bg px-2 text-[12.5px]',
          !folder && 'text-faint'
        ]}
        title={folder ?? undefined}
      >
        {folder ?? 'No folder chosen'}
      </span>
      <button
        type="button"
        disabled={busy}
        class="flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-raised px-3 text-[12.5px] hover:bg-hover disabled:opacity-40"
        onclick={chooseFolder}
      >
        <FolderOpen class="size-3.5" />
        {folder ? 'Change…' : 'Choose…'}
      </button>
    </div>

    {#if scanning}
      <p class="mt-4 flex items-center gap-2 text-[12.5px] text-muted">
        <LoaderCircle class="size-3.5 animate-spin" />
        Searching {folder}…
      </p>
    {:else if scan}
      <p class="mt-4 text-[12.5px] text-muted">
        Found {found.length} of {plural(scan.matches.length, 'track')} in {plural(scan.scanned, 'file')}.
      </p>
      <div class="mt-1.5 max-h-64 overflow-y-auto rounded-md border border-line">
        {#each scan.matches as match (match.id)}
          {@const track = library.byId.get(match.id)}
          <label
            class={['flex items-center gap-2 px-2 py-1.5 text-[12.5px]', match.path ? 'hover:bg-hover' : 'opacity-70']}
          >
            <input
              type="checkbox"
              checked={!!match.path && !skipped.has(match.id)}
              disabled={!match.path || applying}
              onchange={() => toggle(match.id)}
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{track?.title ?? match.id}</span>
              <span class={['block truncate text-[11px]', match.path ? 'text-faint' : 'text-warn']} title={match.path}>
                {match.path ?? 'Not found'}
              </span>
            </span>
          </label>
        {:else}
          <p class="px-2 py-2 text-[12.5px] text-faint">None of these tracks are missing any more</p>
        {/each}
      </div>
    {/if}

    {#if error}
      <p class="mt-3 text-[12.5px] text-danger">{error}</p>
    {/if}

    <div class="mt-5 flex justify-end gap-2">
      <button
        value="cancel"
        disabled={applying}
        class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
      >
        Cancel
      </button>
      <button
        value="confirm"
        disabled={busy || !chosen.length}
        class="h-7 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
      >
        {applying ? 'Relocating…' : chosen.length ? `Relocate ${plural(chosen.length, 'track')}` : 'Relocate'}
      </button>
    </div>
  </form>
</dialog>
