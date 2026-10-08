<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { getApp } from '$lib/context'
  import { buildTree, type TreeNode } from '$lib/library/tree'

  let { playlistId, onclose }: { playlistId: string; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  const tree = $derived(buildTree(library.playlists))

  function collect(node: TreeNode, into: Set<string>): void {
    if (node.kind !== 'folder') into.add(node.id)
    for (const child of node.children) collect(child, into)
  }

  function find(nodes: TreeNode[], id: string): TreeNode | undefined {
    for (const node of nodes) {
      if (node.id === id) return node
      const nested = find(node.children, id)
      if (nested) return nested
    }
    return undefined
  }

  function selectionFor(id: string): SvelteSet<string> {
    const next = new SvelteSet<string>()
    const node = find(tree, id)
    if (node) collect(node, next)
    return next
  }

  let volumeChoice = $state<string | undefined>(undefined)
  let chosen = $state<SvelteSet<string> | null>(null)
  let error = $state<string | null>(null)

  const volumeId = $derived(volumeChoice ?? library.devices[0]?.id ?? '')
  const checked = $derived(chosen ?? selectionFor(playlistId))

  const rows = $derived.by(() => {
    const flat: { node: TreeNode; depth: number }[] = []
    const walk = (nodes: TreeNode[], depth: number) => {
      for (const node of nodes) {
        if (node.kind !== 'folder') flat.push({ node, depth })
        walk(node.children, depth + 1)
      }
    }
    walk(tree, 0)
    return flat
  })

  function phaseLabel(): string {
    const progress = library.syncProgress
    if (!progress) return ''
    if (progress.phase === 'copy') {
      const done = progress.done ?? 0
      const total = progress.total ?? 0
      return total ? `Copying ${done + 1} of ${total}` : 'Copying audio'
    }
    if (progress.phase === 'reading') return 'Reading the volume'
    if (progress.phase === 'writing') return 'Writing the library'
    if (progress.phase === 'staged' || progress.phase === 'verifying') return 'Checking the export'
    if (progress.phase === 'publishing') return 'Publishing'
    if (progress.phase === 'done') return 'Sync finished'
    return 'Syncing'
  }

  function toggle(id: string): void {
    const next = new SvelteSet(checked)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    chosen = next
  }

  function modal(dialog: HTMLDialogElement): () => void {
    const resume = keybinds.suspend()
    dialog.showModal()
    return resume
  }

  async function submit(dialog: HTMLDialogElement): Promise<void> {
    if (library.syncing || !volumeId || checked.size === 0) return
    error = null
    const ok = await library.syncTo(volumeId, [...checked])
    if (ok) dialog.close()
    else error = library.syncError
  }
</script>

<dialog
  {@attach modal}
  class="no-drag m-auto w-[420px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Sync to a volume"
  onclose={() => onclose()}
  onclick={(e) => {
    if (library.syncing) return
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
    <h2 class="text-[13.5px] font-semibold">Sync to a volume</h2>
    <p class="mt-1.5 text-[12.5px] leading-relaxed text-muted">
      Copy the selected playlists onto a rekordbox volume.
    </p>

    <label class="mt-4 block text-[11px] font-semibold tracking-wider text-faint uppercase" for="sync-volume">
      Volume
    </label>
    {#if library.devices.length}
      <select
        id="sync-volume"
        value={volumeId}
        disabled={library.syncing}
        class="mt-1.5 h-8 w-full rounded-md border border-line bg-bg px-2 text-[12.5px]"
        onchange={(e) => (volumeChoice = e.currentTarget.value)}
      >
        {#each library.devices as volume (volume.id)}
          <option value={volume.id}>{volume.name}{volume.hasLibrary ? '' : ' · empty'}</option>
        {/each}
      </select>
    {:else}
      <p class="mt-1.5 text-[12.5px] text-muted">Connect a USB drive to choose a destination.</p>
    {/if}

    <p class="mt-4 text-[11px] font-semibold tracking-wider text-faint uppercase">Playlists</p>
    <div class="mt-1.5 max-h-56 overflow-y-auto rounded-md border border-line">
      {#each rows as row (row.node.id)}
        <label
          class="flex h-7 items-center gap-2 pr-2 text-[12.5px] hover:bg-hover"
          style:padding-left="{8 + row.depth * 14}px"
        >
          <input
            type="checkbox"
            checked={checked.has(row.node.id)}
            disabled={library.syncing}
            onchange={() => toggle(row.node.id)}
          />
          <span class="min-w-0 flex-1 truncate">{row.node.name || 'Untitled'}</span>
        </label>
      {:else}
        <p class="px-2 py-2 text-[12.5px] text-faint">No playlists yet</p>
      {/each}
    </div>

    {#if library.syncing}
      <p class="mt-3 text-[12.5px] text-muted">{phaseLabel()}</p>
    {/if}
    {#if error}
      <p class="mt-3 text-[12.5px] text-danger">{error}</p>
    {/if}

    <div class="mt-5 flex justify-end gap-2">
      <button
        value="cancel"
        disabled={library.syncing}
        class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
      >
        Cancel
      </button>
      <button
        value="confirm"
        disabled={library.syncing || !volumeId || checked.size === 0}
        class="h-7 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
      >
        Sync
      </button>
    </div>
  </form>
</dialog>
