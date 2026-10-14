<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import { buildTree, type TreeNode } from '$lib/library/tree'
  import type { TreeItem } from '$lib/types'

  let { playlistId, onclose }: { playlistId: string; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  const tree = $derived(buildTree(library.playlists))

  let volumeChoice = $state<string | undefined>(undefined)
  let userTicks = $state<SvelteSet<string> | null>(null)
  let recorded = $state.raw<string[]>([])
  let deviceRows = $state.raw<TreeItem[]>([])
  let loadedFor = $state<string | null>(null)
  let error = $state<string | null>(null)

  const volumeId = $derived(volumeChoice ?? library.devices[0]?.id ?? '')
  const pending = $derived(volumeId !== '' && loadedFor !== volumeId)

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

  function leaves(node: TreeNode): string[] {
    const ids = new Set<string>()
    collect(node, ids)
    return [...ids]
  }

  function baseSelection(): SvelteSet<string> {
    const next = new SvelteSet<string>()
    for (const id of recorded) {
      const node = find(tree, id)
      if (node) collect(node, next)
    }
    const context = find(tree, playlistId)
    if (context) collect(context, next)
    return next
  }

  const checked = $derived.by(() => {
    if (loadedFor !== volumeId) return new SvelteSet<string>()
    return userTicks ?? baseSelection()
  })

  function flatten(nodes: TreeNode[]): { node: TreeNode; depth: number }[] {
    const flat: { node: TreeNode; depth: number }[] = []
    const walk = (list: TreeNode[], depth: number) => {
      for (const node of list) {
        flat.push({ node, depth })
        walk(node.children, depth + 1)
      }
    }
    walk(nodes, 0)
    return flat
  }

  const rows = $derived(flatten(tree))
  const deviceTree = $derived(buildTree(deviceRows))
  const deviceFlat = $derived(flatten(deviceTree))

  function folderState(node: TreeNode): 'on' | 'off' | 'mixed' {
    const ids = leaves(node)
    if (!ids.length) return 'off'
    const count = ids.filter((id) => checked.has(id)).length
    if (count === 0) return 'off'
    if (count === ids.length) return 'on'
    return 'mixed'
  }

  function indeterminate(mixed: boolean) {
    return (node: HTMLInputElement) => {
      node.indeterminate = mixed
    }
  }

  function loadSelection(id: string) {
    return () => {
      let cancelled = false
      userTicks = null
      recorded = []
      deviceRows = []
      error = null
      if (!id) {
        loadedFor = ''
        return () => {}
      }
      loadedFor = null
      void (async () => {
        try {
          const [selection, onDevice] = await Promise.all([
            api.usbSelection(id),
            api.usbPlaylists(id).catch((reason: unknown) => {
              const message = reason instanceof Error ? reason.message : ''
              if (!message.toLowerCase().includes('no rekordbox library')) throw reason
              return [] as TreeItem[]
            })
          ])
          if (cancelled) return
          recorded = selection.playlistIds
          deviceRows = onDevice
          loadedFor = id
        } catch (reason) {
          if (cancelled) return
          recorded = []
          deviceRows = []
          loadedFor = id
          error = reason instanceof Error ? reason.message : 'Could not read this volume'
        }
      })()
      return () => {
        cancelled = true
      }
    }
  }

  function toggleLeaf(id: string): void {
    const next = new SvelteSet(checked)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    userTicks = next
  }

  function toggleFolder(node: TreeNode): void {
    const ids = leaves(node)
    if (!ids.length) return
    const next = new SvelteSet(checked)
    const all = ids.every((id) => next.has(id))
    for (const id of ids) {
      if (all) next.delete(id)
      else next.add(id)
    }
    userTicks = next
  }

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

  async function submit(dialog: HTMLDialogElement): Promise<void> {
    if (library.syncing || pending || !volumeId || checked.size === 0) return
    error = null
    const ok = await library.syncTo(volumeId, [...checked])
    if (ok) dialog.close()
    else error = library.syncError
  }
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto w-[680px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Sync to a volume"
  onclose={() => onclose()}
  onclick={(e) => {
    if (library.syncing) return
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <form
    method="dialog"
    {@attach loadSelection(volumeId)}
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
      The ticked playlists are what this sync writes. An unticked playlist from this library is removed from the stick.
      Playlists that are not in this library stay there.
    </p>

    <div class="mt-4 grid grid-cols-2 gap-4">
      <div class="min-w-0">
        <p class="text-[11px] font-semibold tracking-wider text-faint uppercase">Playlists</p>
        <div class="mt-1.5 max-h-64 overflow-y-auto rounded-md border border-line">
          {#each rows as row (row.node.id)}
            {@const state = row.node.kind === 'folder' ? folderState(row.node) : checked.has(row.node.id) ? 'on' : 'off'}
            <label
              class="flex h-7 items-center gap-2 pr-2 text-[12.5px] hover:bg-hover"
              style:padding-left="{8 + row.depth * 14}px"
            >
              <input
                type="checkbox"
                checked={state === 'on'}
                {@attach indeterminate(state === 'mixed')}
                disabled={library.syncing || pending || (row.node.kind === 'folder' && leaves(row.node).length === 0)}
                onchange={() => (row.node.kind === 'folder' ? toggleFolder(row.node) : toggleLeaf(row.node.id))}
              />
              <span class="min-w-0 flex-1 truncate">{row.node.name || 'Untitled'}</span>
            </label>
          {:else}
            <p class="px-2 py-2 text-[12.5px] text-faint">No playlists yet</p>
          {/each}
        </div>
      </div>

      <div class="min-w-0">
        <label class="block text-[11px] font-semibold tracking-wider text-faint uppercase" for="sync-volume">
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

        <p class="mt-4 text-[11px] font-semibold tracking-wider text-faint uppercase">On this volume</p>
        <div class="mt-1.5 max-h-48 overflow-y-auto rounded-md border border-line">
          {#if pending}
            <p class="px-2 py-2 text-[12.5px] text-faint">Reading this volume</p>
          {:else}
            {#each deviceFlat as row (row.node.id)}
              <div
                class="flex h-7 items-center gap-2 pr-2 text-[12.5px]"
                style:padding-left="{8 + row.depth * 14}px"
              >
                <span class="min-w-0 flex-1 truncate text-muted">{row.node.name || 'Untitled'}</span>
                {#if row.node.kind !== 'folder' && row.node.count != null}
                  <span class="shrink-0 text-[11px] text-faint">{row.node.count}</span>
                {/if}
              </div>
            {:else}
              <p class="px-2 py-2 text-[12.5px] text-faint">Nothing on this volume yet</p>
            {/each}
          {/if}
        </div>
      </div>
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
        disabled={library.syncing || pending || !volumeId || checked.size === 0}
        class="h-7 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
      >
        Sync
      </button>
    </div>
  </form>
</dialog>
