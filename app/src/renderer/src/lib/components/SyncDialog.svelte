<script lang="ts">
  import { Check, ChevronRight, Eraser, Folder, FolderOpen, Library, ListMusic, LoaderCircle, RefreshCw, Search, Sparkles, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { loadPresets } from '$lib/convertPresetStore'
  import { FORMAT_NAMES, presetKey, presetLabel } from '$lib/convertPresets'
  import { suspendModal } from '$lib/modal'
  import { buildTree, type TreeNode } from '$lib/library/tree'
  import type { DeviceLibraryView, Volume } from '$lib/types'
  import type { SyncInspection, SyncPlan, SyncResolution } from '../../../../preload/engine'

  const COLLECTION_ID = 'collection'

  let { playlistId = '', volumeId: preferredVolume = '', onclose }: { playlistId?: string; volumeId?: string; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  const tree = $derived(library.playlistTree)
  const collectionRoot = $derived<TreeNode>({
    id: COLLECTION_ID,
    name: 'Collection',
    parentId: '',
    seq: 0,
    kind: 'folder',
    count: library.collectionSize,
    children: tree
  })

  /** Null until the user toggles a device, so the first connected drive starts selected. */
  let picked = $state<string[] | null>(null)
  let userTicks = $state<SvelteSet<string> | null>(null)
  let recorded = $state.raw<string[]>([])
  let deviceLibraries = $state.raw<Record<string, DeviceLibraryView[]>>({})
  let loadedFor = $state<string | null>(null)
  const open = new SvelteSet<string>([`playlist:${COLLECTION_ID}`])
  const seenLibraries = new SvelteSet<string>()
  let error = $state<string | null>(null)
  let notice = $state<string | null>(null)
  let query = $state('')
  let ejectAfter = $state(false)
  let refreshToken = $state(0)
  let deviceToken = $state(0)
  let confirm = $state<'format' | null>(null)
  let formatId = $state<string | null>(null)
  let batch = $state(false)
  let ejecting = $state(false)
  let activeDevice = $state<string | null>(null)
  let inspections = $state.raw<Record<string, SyncInspection>>({})
  let inspectionErrors = $state.raw<Record<string, string>>({})
  let resolutions = $state<Record<string, Record<string, SyncResolution>>>({})
  let reviewing = $state(false)
  const showInformation = new SvelteSet<string>()
  const presets = loadPresets()
  /** The chosen format by preset key; empty copies every track as it is. */
  let convertKey = $state('')
  const convert = $derived(presets.find((preset) => presetKey(preset) === convertKey))

  const selectedIds = $derived.by(() => {
    const present = new Set(library.devices.map((item) => item.id))
    const fallback = preferredVolume ? [preferredVolume] : library.devices[0] ? [library.devices[0].id] : []
    const chosen = picked ?? fallback
    return chosen.filter((id) => present.has(id))
  })
  const volumeId = $derived(selectedIds[0] ?? '')
  const pending = $derived(volumeId !== '' && loadedFor !== volumeId)
  const locked = $derived(library.syncing || batch)
  const canEject = $derived(selectedIds.some((id) => library.devices.some((item) => item.id === id && item.ejectable)))

  async function recoverVolume(id: string): Promise<void> {
    if (locked) return
    error = null
    try {
      await api.usbRecover(id)
      refreshToken += 1
      deviceToken += 1
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'Could not recover USB publication'
    }
  }

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

  const reviewKey = $derived(JSON.stringify([selectedIds, [...checked].sort(), refreshToken, convertKey]))
  const reviewBlocked = $derived(selectedIds.some((id) => inspections[id]?.diagnostics.some((item) => item.severity === 'blocked')))
  const reviewUnresolved = $derived(selectedIds.some((id) => {
    const inspection = inspections[id]
    return !inspection || inspection.diagnostics.some((item) => item.severity === 'decision_required' && !resolutions[id]?.[item.id])
  }))

  $effect(() => {
    const key = reviewKey
    const ids = untrack(() => [...selectedIds])
    const playlistIds = untrack(() => [...checked].sort())
    const preset = untrack(() => convert)
    if (!ids.length || pending || !playlistIds.length) {
      inspections = {}
      inspectionErrors = {}
      resolutions = {}
      reviewing = false
      return undefined
    }
    let cancelled = false
    reviewing = true
    inspections = {}
    inspectionErrors = {}
    resolutions = {}
    void Promise.all(ids.map(async (id) => {
      try {
        return [id, await api.usbSyncReview(id, playlistIds, preset)] as const
      } catch (reason) {
        const message = reason instanceof Error ? reason.message : 'Could not inspect USB changes'
        return [id, message] as const
      }
    })).then((results) => {
      if (cancelled || key !== reviewKey) return
      const nextInspections: Record<string, SyncInspection> = {}
      const nextErrors: Record<string, string> = {}
      for (const [id, result] of results) {
        if (typeof result === 'string') nextErrors[id] = result
        else nextInspections[id] = result
      }
      inspections = nextInspections
      inspectionErrors = nextErrors
    }).finally(() => {
      if (!cancelled && key === reviewKey) reviewing = false
    })
    return (): void => {
      cancelled = true
    }
  })

  function chooseResolution(volume: string, diagnostic: string, resolution: SyncResolution): void {
    resolutions = {
      ...resolutions,
      [volume]: { ...resolutions[volume], [diagnostic]: resolution }
    }
  }

  function resolutionLabel(resolution: SyncResolution): string {
    switch (resolution) {
      case 'use_library': return 'Use library version'
      case 'restore_from_library': return 'Restore from library'
      case 'discard_device_edits': return 'Discard device edits'
    }
  }

  function leafStats(nodes: TreeNode[], selected: SvelteSet<string>): { total: number; picked: number } {
    let total = 0
    let picked = 0
    const walk = (list: TreeNode[]): void => {
      for (const node of list) {
        if (node.kind !== 'folder') {
          total += 1
          if (selected.has(node.id)) picked += 1
        }
        walk(node.children)
      }
    }
    walk(nodes)
    return { total, picked }
  }

  const stats = $derived(leafStats(tree, checked))
  const needle = $derived(query.trim().toLowerCase())

  function nodeMatches(node: TreeNode, term: string): boolean {
    if (!term) return true
    if ((node.name || 'Untitled').toLowerCase().includes(term)) return true
    return node.children.some((child) => nodeMatches(child, term))
  }

  const searchHit = $derived(!needle || tree.some((node) => nodeMatches(node, needle)))

  function ancestorKeys(nodes: TreeNode[], ids: { has(id: string): boolean }, prefix: string): string[] {
    const keys: string[] = []
    const walk = (list: TreeNode[]): boolean => {
      let hit = false
      for (const node of list) {
        const here = ids.has(node.id) || walk(node.children)
        if (here && node.kind === 'folder') keys.push(`${prefix}:${node.id}`)
        if (here) hit = true
      }
      return hit
    }
    walk(nodes)
    return keys
  }
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

  function loadSelection(id: string, token: number) {
    return () => {
      let cancelled = false
      // Reading loadedFor directly would rerun this loader, which clears it, so the checkboxes stay disabled.
      const sameVolume = untrack(() => loadedFor) === id
      if (!sameVolume) {
        userTicks = null
        error = null
        if (playlistId) {
          untrack(() => {
            for (const key of ancestorKeys(tree, new SvelteSet([playlistId]), 'playlist')) open.add(key)
          })
        }
      }
      recorded = []
      void token
      if (!id) {
        loadedFor = ''
        return () => {}
      }
      loadedFor = null
      void (async () => {
        try {
          const selection = await api.usbSelection(id)
          if (cancelled) return
          recorded = selection.playlistIds
          loadedFor = id
          if (!sameVolume) {
            const ids = new SvelteSet(selection.playlistIds)
            if (playlistId) ids.add(playlistId)
            untrack(() => {
              for (const key of ancestorKeys(tree, ids, 'playlist')) open.add(key)
            })
          }
        } catch (reason) {
          if (cancelled) return
          recorded = []
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

  function toggleDevice(id: string): void {
    picked = selectedIds.includes(id) ? [] : [id]
  }

  function share(item: Volume): { used: number; free: number } {
    if (item.bytesTotal <= 0) return { used: 0, free: 0 }
    return {
      used: Math.min(100, (item.bytesUsed / item.bytesTotal) * 100),
      free: Math.max(0, Math.min(100, Math.round((item.bytesFree / item.bytesTotal) * 100)))
    }
  }

  function toggleOpen(key: string): void {
    if (open.has(key)) open.delete(key)
    else open.add(key)
  }

  function loadLibraries(idList: string, token: number) {
    return () => {
      let cancelled = false
      void token
      const ids = idList.split('\n').filter(Boolean)
      void (async () => {
        const settled = await Promise.all(
          ids.map(async (id) => {
            try {
              return [id, await api.usbLibraries(id)] as const
            } catch (reason: unknown) {
              const message = reason instanceof Error ? reason.message : ''
              if (!message.toLowerCase().includes('no rekordbox library') && !cancelled) {
                error = message || 'Could not read this volume'
              }
              return [id, [] as DeviceLibraryView[]] as const
            }
          })
        )
        if (cancelled) return
        deviceLibraries = Object.fromEntries(settled)
        untrack(() => {
          for (const [volume, libs] of settled) {
            for (const lib of libs) {
              const key = `${volume}:${lib.id}`
              if (!seenLibraries.has(key)) {
                seenLibraries.add(key)
                open.add(key)
              }
            }
          }
        })
      })()
      return () => {
        cancelled = true
      }
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes <= 0) return '0 B'
    const units = ['B', 'KB', 'MB', 'GB', 'TB']
    let value = bytes
    let unit = 0
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024
      unit += 1
    }
    const digits = unit === 0 || value >= 100 ? 0 : 1
    return `${value.toFixed(digits)} ${units[unit]}`
  }

  function planChanges(plan: SyncPlan): string {
    if (plan.upToDate) return 'Up to date · no changes'
    const parts: string[] = []
    if (plan.convert) parts.push(`${plan.convert} to convert`)
    if (plan.copy) parts.push(`${plan.copy} to copy`)
    if (plan.remove) parts.push(`${plan.remove} to remove`)
    return parts.length ? parts.join(' · ') : 'No audio to copy or convert'
  }

  function phaseLabel(): string {
    const device = activeDevice ? library.devices.find((item) => item.id === activeDevice) : null
    const prefix = device && selectedIds.length > 1 ? `${device.name} · ` : ''
    if (ejecting) return prefix + 'Ejecting device'
    const progress = library.syncProgress
    if (!progress) return prefix + 'Preparing sync'
    if (progress.phase === 'copy') {
      const done = progress.done ?? 0
      const total = progress.total ?? 0
      return prefix + (total ? `Copying ${done + 1} of ${total}` : 'Copying audio')
    }
    if (progress.phase === 'convert') {
      const done = progress.done ?? 0
      const total = progress.total ?? 0
      const name = progress.name ? ` · ${progress.name}` : ''
      return prefix + (total ? `Converting ${done + 1} of ${total}${name}` : 'Converting audio')
    }
    if (progress.phase === 'import') {
      const done = progress.done ?? 0
      const total = progress.total ?? 0
      const name = progress.name ? ` · ${progress.name}` : ''
      return prefix + (total ? `Importing ${Math.min(done + 1, total)} of ${total}${name}` : 'Importing')
    }
    if (progress.phase === 'format') return 'Formatting the USB'
    if (progress.phase === 'reading') return prefix + 'Reading the volume'
    if (progress.phase === 'writing') return prefix + 'Writing the library'
    if (progress.phase === 'staged' || progress.phase === 'verifying') return prefix + 'Checking the export'
    if (progress.phase === 'publishing') return prefix + 'Publishing'
    if (progress.phase === 'done') return prefix + 'Finalizing sync'
    return prefix + 'Syncing'
  }

  async function submit(): Promise<void> {
    const targets = [...selectedIds]
    const playlistIds = [...checked]
    if (locked || pending || reviewing || reviewBlocked || reviewUnresolved || !targets.length || !playlistIds.length) return
    error = null
    notice = null
    confirm = null
    batch = true
    let ejected = 0
    try {
      for (const id of targets) {
        activeDevice = id
        const inspection = inspections[id]
        const selectedResolutions = resolutions[id] ?? {}
        const review = inspection && Object.keys(selectedResolutions).length
          ? { token: inspection.token, resolutions: { ...selectedResolutions } }
          : undefined
        const ok = await library.syncTo(id, playlistIds, review, convert)
        if (!ok) {
          error = library.syncError
          refreshToken += 1
          return
        }
      }
      deviceToken += 1
      refreshToken += 1
      if (ejectAfter) {
        for (const id of targets) {
          const device = library.devices.find((item) => item.id === id)
          if (!device?.ejectable) continue
          try {
            activeDevice = id
            ejecting = true
            await library.ejectDevice(id)
            ejected += 1
          } catch (reason) {
            error = reason instanceof Error ? reason.message : 'Couldn’t eject the device'
            return
          }
        }
      }
      notice = ejected
        ? 'Sync finished and the device was ejected'
        : 'Sync finished'
    } finally {
      ejecting = false
      activeDevice = null
      batch = false
    }
  }

  async function runFormat(): Promise<void> {
    const id = formatId
    if (locked || !id) return
    confirm = null
    error = null
    notice = null
    const ok = await library.formatDevice(id)
    if (!ok) {
      error = library.syncError
      return
    }
    formatId = null
    deviceToken += 1
    if (id === volumeId) {
      userTicks = null
      refreshToken += 1
    }
    notice = 'Formatted the USB'
  }
</script>

{#snippet leafIcon(node: TreeNode)}
  {#if node.kind === 'smart'}
    <Sparkles class="size-3.5 shrink-0 text-accent/80" />
  {:else}
    <ListMusic class="size-3.5 shrink-0 text-faint" />
  {/if}
{/snippet}

{#snippet playlistTree(nodes: TreeNode[], depth: number, prefix: string, selectable: boolean, term: string)}
  {#each nodes as node (node.id)}
    {#if !term || nodeMatches(node, term)}
      {@const key = `${prefix}:${node.id}`}
      {@const isFolder = node.kind === 'folder'}
      {@const collection = node.id === COLLECTION_ID}
      {@const named = (node.name || 'Untitled').toLowerCase().includes(term)}
      {@const opened = isFolder && ((term !== '' && (named || node.children.some((child) => nodeMatches(child, term)))) || open.has(key))}
      {@const state = isFolder ? folderState(node) : checked.has(node.id) ? 'on' : 'off'}
      <div
        class={[
          'flex items-center gap-1.5 rounded-md pr-2 hover:bg-hover hover:text-fg',
          collection ? 'h-8 text-[13px] font-medium text-fg' : 'h-7 text-[12.5px] text-muted'
        ]}
        style:padding-left="{8 + depth * 14}px"
      >
        {#if selectable && isFolder}
          <input
            type="checkbox"
            class="shrink-0"
            checked={state === 'on'}
            {@attach indeterminate(state === 'mixed')}
            disabled={locked || pending || leaves(node).length === 0}
            aria-label={`Select playlists in ${node.name || 'folder'}`}
            aria-checked={state === 'mixed' ? 'mixed' : undefined}
            onchange={() => toggleFolder(node)}
          />
        {/if}
        {#if isFolder}
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-1.5 text-left"
            aria-expanded={opened}
            onclick={() => toggleOpen(key)}
          >
            <ChevronRight class={['size-3 shrink-0 text-faint transition-transform', opened && 'rotate-90']} />
            {#if collection}
              <Library class="size-4 shrink-0 text-accent" />
            {:else if opened}
              <FolderOpen class="size-3.5 shrink-0 text-faint" />
            {:else}
              <Folder class="size-3.5 shrink-0 text-faint" />
            {/if}
            <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
            {#if node.count != null}
              <span class="tabular shrink-0 text-[11px] font-normal text-faint">{node.count}</span>
            {/if}
          </button>
        {:else if selectable}
          <label class="flex min-w-0 flex-1 items-center gap-1.5">
            <input
              type="checkbox"
              class="shrink-0"
              checked={state === 'on'}
              disabled={locked || pending}
              aria-label={`Select ${node.name || 'playlist'}`}
              onchange={() => toggleLeaf(node.id)}
            />
            <span class="w-3 shrink-0"></span>
            {@render leafIcon(node)}
            <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
            {#if node.count != null}
              <span class="tabular shrink-0 text-[11px] text-faint">{node.count}</span>
            {/if}
          </label>
        {:else}
          <span class="w-3 shrink-0"></span>
          {@render leafIcon(node)}
          <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
          {#if node.count != null}
            <span class="tabular shrink-0 text-[11px] text-faint">{node.count}</span>
          {/if}
        {/if}
      </div>
      {#if isFolder && opened}
        {#if collection && !node.children.length}
          <p class="py-1 pr-2 text-[12px] text-faint" style:padding-left="{8 + (depth + 1) * 14}px">No playlists yet</p>
        {/if}
        {@render playlistTree(node.children, depth + 1, prefix, selectable, named ? '' : term)}
      {/if}
    {/if}
  {/each}
{/snippet}

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto w-[820px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Sync"
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
    {@attach loadSelection(volumeId, refreshToken)}
    class="flex h-[min(760px,calc(100dvh-48px))] flex-col overflow-hidden"
    onsubmit={(e) => {
      if (e.submitter?.getAttribute('value') !== 'confirm') return
      e.preventDefault()
      void submit()
    }}
  >
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
      <h2 class="min-w-0 flex-1 text-[13px] font-semibold">Sync</h2>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
        aria-label="Close"
        disabled={locked}
        onclick={(e) => {
          if (locked) return
          e.currentTarget.closest('dialog')?.close()
        }}
      >
        <X class="size-4" />
      </button>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden p-5">
    <div class="grid h-full min-h-0 grid-cols-2 gap-4">
      <section class="flex min-h-0 min-w-0 flex-col">
        <h2 class="text-[13.5px] font-semibold">Playlists</h2>
        <p class="mt-1 text-[12px] text-muted">
          {#if pending}
            Reading this volume
          {:else}
            {stats.picked} of {stats.total} playlists selected
          {/if}
        </p>
        <label class="mt-3 flex h-8 shrink-0 items-center gap-2 rounded-md border border-line bg-bg px-2">
          <Search class="size-3.5 shrink-0 text-faint" />
          <input
            type="search"
            bind:value={query}
            placeholder="Search playlists"
            disabled={locked}
            class="min-w-0 flex-1 bg-transparent text-[12.5px] outline-none placeholder:text-faint"
            onkeydown={(e) => {
              if (e.key === 'Enter') e.preventDefault()
            }}
          />
        </label>
        <div class="mt-2 min-h-0 flex-1 overflow-y-auto overscroll-contain rounded-md border border-line py-1">
          {#if needle && !searchHit}
            <p class="px-2 py-1 text-[12.5px] text-faint">No matching playlists</p>
          {:else}
            {@render playlistTree([collectionRoot], 0, 'playlist', true, needle)}
          {/if}
        </div>
        <p class="mt-2 text-[11px] leading-relaxed text-faint">
          Unticked playlists from this library are removed from the USB. Playlists that aren’t in this library stay.
        </p>
      </section>

      <section class="flex min-h-0 min-w-0 flex-col" {@attach loadLibraries(library.devices.map((item) => item.id).join('\n'), deviceToken)}>
        <div class="flex shrink-0 items-baseline justify-between gap-3">
          <h2 class="text-[13.5px] font-semibold">Devices</h2>
          <button
            type="button"
            class="inline-flex h-6 items-center gap-1 rounded-md border border-line px-2 text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
            disabled={locked}
            onclick={() => {
              void library.refreshDevices()
              refreshToken += 1
              deviceToken += 1
            }}
          >
            <RefreshCw class="size-3" />
            Refresh
          </button>
        </div>
        <p class="mt-1 text-[12px] text-muted">
          {selectedIds.length} of {library.devices.length} selected
        </p>

        {#if library.devices.length}
          <div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain rounded-md border border-line">
            {#each library.devices as item (item.id)}
              {@const libs = deviceLibraries[item.id]}
              {@const chosen = selectedIds.includes(item.id)}
              {@const usage = share(item)}
              {@const detail = [item.fileSystem, item.bytesTotal > 0 ? formatBytes(item.bytesTotal) : ''].filter(Boolean).join(' · ')}
              <div class="border-t border-line first:border-t-0">
                <div class="flex items-center gap-1.5 px-2 py-1">
                  <label class="flex h-7 min-w-0 flex-1 items-center gap-2 text-[12.5px] hover:text-fg">
                    <input
                      type="checkbox"
                      class="shrink-0"
                      checked={chosen}
                      disabled={locked}
                      aria-label={`Sync to ${item.name}`}
                      onchange={() => toggleDevice(item.id)}
                    />
                    <span class="min-w-0 flex-1 truncate">{item.name}</span>
                    {#if detail}
                      <span class="max-w-[46%] shrink-0 truncate text-[11px] text-faint">{detail}</span>
                    {/if}
                  </label>
                  {#if item.ejectable}
                    <button
                      type="button"
                      class="inline-flex h-6 shrink-0 items-center gap-1 rounded-md border border-line px-1.5 text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
                      disabled={locked}
                      onclick={() => {
                        formatId = item.id
                        confirm = 'format'
                      }}
                    >
                      <Eraser class="size-3" />
                      Format USB
                    </button>
                  {/if}
                </div>
                {#if confirm === 'format' && formatId === item.id}
                  <div class="border-t border-line px-2.5 py-2.5">
                    <p class="text-[12.5px] leading-relaxed text-muted">
                      Erase everything on {item.name} and format it as FAT32? This cannot be undone.
                    </p>
                    <div class="mt-2.5 flex justify-end gap-2">
                      <button
                        type="button"
                        class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg"
                        onclick={() => (confirm = null)}
                      >
                        Cancel
                      </button>
                      <button
                        type="button"
                        class="h-7 rounded-md bg-danger px-3 text-[12.5px] font-medium text-bg hover:bg-danger/90"
                        onclick={() => void runFormat()}
                      >
                        Format USB
                      </button>
                    </div>
                  </div>
                {/if}
                {#if chosen && item.bytesTotal > 0}
                  <div class="px-2.5 pb-2">
                    <div class="h-1.5 overflow-hidden rounded-full bg-line">
                      <div class="h-full bg-accent" style:width="{usage.used}%"></div>
                    </div>
                    <p class="mt-1.5 flex justify-between gap-3 text-[11px] text-faint">
                      <span>{formatBytes(item.bytesUsed)} used</span>
                      <span>{formatBytes(item.bytesFree)} free ({usage.free}%)</span>
                    </p>
                  </div>
                {/if}
                <div class="max-h-40 overflow-y-auto overscroll-contain" role="region" aria-label={`Playlists on ${item.name}`}>
                {#if libs === undefined}
                  <p class="px-2 pb-2 text-[12px] text-faint">Reading this volume</p>
                {:else if libs.length === 0}
                  <p class="px-2 pb-2 text-[12px] text-faint">Nothing on this volume yet</p>
                {:else}
                  {#each libs as lib (lib.id)}
                    {@const libraryKey = `${item.id}:${lib.id}`}
                    {@const libraryOpen = open.has(libraryKey)}
                    <button
                      type="button"
                      class="flex h-7 w-full items-center gap-1.5 px-2 text-left text-[12.5px] text-fg hover:bg-hover"
                      aria-expanded={libraryOpen}
                      onclick={() => toggleOpen(libraryKey)}
                    >
                      <ChevronRight class={['size-3 shrink-0 text-faint transition-transform', libraryOpen && 'rotate-90']} />
                      {#if libraryOpen}
                        <FolderOpen class="size-3.5 shrink-0 text-faint" />
                      {:else}
                        <Folder class="size-3.5 shrink-0 text-faint" />
                      {/if}
                      <span class="min-w-0 flex-1 truncate">{lib.name}</span>
                    </button>
                    {#if libraryOpen}
                      {#if lib.playlists.length}
                        {@render playlistTree(buildTree(lib.playlists), 1, libraryKey, false, '')}
                      {:else}
                        <p class="py-1 pr-2 text-[12px] text-faint" style:padding-left="36px">No playlists</p>
                      {/if}
                    {/if}
                  {/each}
                {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <p class="mt-3 text-[12.5px] text-muted">Connect a USB drive.</p>
        {/if}

        {#if error}
          <p class="mt-2.5 text-[12px] leading-relaxed text-warn">{error}</p>
        {/if}

        <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {#if reviewing}
          <p class="mt-3 flex items-center gap-2 border-t border-line pt-3 text-[12px] text-muted" role="status">
            <LoaderCircle class="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            Reviewing USB changes
          </p>
        {:else}
          {#each selectedIds as id (id)}
            {@const inspection = inspections[id]}
            {@const deviceName = library.devices.find((item) => item.id === id)?.name ?? id}
            {#if inspectionErrors[id]}
              <p class="mt-3 border-t border-line pt-3 text-[12px] leading-relaxed text-warn">
                {deviceName}: {inspectionErrors[id]}
              </p>
            {:else if inspection}
              {@const informationCount = inspection.diagnostics.filter((item) => item.severity === 'information').length}
              {@const visibleDiagnostics = inspection.diagnostics.filter((item) => item.severity !== 'information' || showInformation.has(id))}
              <section class="mt-3 border-t border-line pt-3" aria-label={`USB changes on ${deviceName}`}>
                <h3 class="text-[12.5px] font-semibold">Summary · {deviceName}</h3>
                {#if inspection.plan}
                  <p class="mt-0.5 text-[11px] text-faint">
                    {inspection.plan.tracks} tracks · {inspection.plan.playlists} playlists
                    {inspection.plan.estimatedPeakBytes == null ? ' · space estimate unavailable' : ` · ${inspection.plan.approximate ? 'approximately ' : ''}up to ${formatBytes(inspection.plan.estimatedPeakBytes)}`}
                  </p>
                  <p class="mt-0.5 text-[11px] text-faint">{planChanges(inspection.plan)}</p>
                {/if}
                {#if informationCount > 0}
                  <label class="mt-2 flex items-center gap-2 text-[11px] text-muted">
                    <input
                      type="checkbox"
                      checked={showInformation.has(id)}
                      onchange={(event) => {
                        if (event.currentTarget.checked) showInformation.add(id)
                        else showInformation.delete(id)
                      }}
                    />
                    Show informational changes ({informationCount})
                  </label>
                {/if}
                <div class="mt-2 flex max-h-48 flex-col gap-2 overflow-y-auto overscroll-contain" role="region" aria-label={`Change details for ${deviceName}`}>
                  {#if visibleDiagnostics.length === 0}
                    <p class="text-[11px] text-muted">No device changes need review.</p>
                  {/if}
                  {#each visibleDiagnostics as diagnostic (diagnostic.id)}
                    <div class="shrink-0 break-words border-l-2 {diagnostic.severity === 'blocked' ? 'border-danger' : diagnostic.severity === 'decision_required' ? 'border-warn' : 'border-line'} pl-2.5">
                      <p class="text-[12px] font-medium">{diagnostic.name || diagnostic.entity}</p>
                      <p class="mt-0.5 text-[11px] leading-relaxed text-muted">{diagnostic.detail}</p>
                      {#if diagnostic.kind === 'recovery_required'}
                        <button
                          type="button"
                          class="mt-1 h-7 rounded-md border border-line px-2 text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
                          disabled={locked}
                          onclick={() => void recoverVolume(id)}
                        >
                          Recover interrupted export
                        </button>
                      {/if}
                      {#if diagnostic.fields.length}
                        <p class="text-[10.5px] text-faint">{diagnostic.fields.join(' · ')}{diagnostic.formats.length ? ` · ${diagnostic.formats.join(' / ')}` : ''}</p>
                      {/if}
                      {#each diagnostic.evidence as evidence (evidence.field)}
                        <p class="break-words text-[10.5px] text-faint">
                          {evidence.field}: baseline {evidence.baseline ?? 'unknown'} · device {evidence.device ?? 'absent'} · library {evidence.library ?? 'unavailable'}
                        </p>
                      {/each}
                      {#if diagnostic.severity === 'decision_required'}
                        <select
                          class="mt-1 h-7 max-w-full rounded-md border border-line bg-bg px-2 text-[11px]"
                          aria-label={`Resolution for ${diagnostic.name}`}
                          value={resolutions[id]?.[diagnostic.id] ?? ''}
                          onchange={(event) => {
                            const value = event.currentTarget.value as SyncResolution | ''
                            if (value) chooseResolution(id, diagnostic.id, value)
                          }}
                        >
                          <option value="">Choose action</option>
                          {#each diagnostic.allowedResolutions as resolution (resolution)}
                            <option value={resolution}>{resolutionLabel(resolution)}</option>
                          {/each}
                        </select>
                      {/if}
                    </div>
                  {/each}
                </div>
              </section>
            {/if}
          {/each}
        {/if}
        </div>

        <div class="mt-3 flex shrink-0 flex-col gap-2.5">
          <label class="flex flex-col gap-1 text-[12px] text-muted">
            Convert audio
            <select
              class="h-8 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
              bind:value={convertKey}
              disabled={locked}
            >
              <option value="">Don't convert</option>
              {#each presets as preset (presetKey(preset))}
                <option value={presetKey(preset)}>{presetLabel(preset)}</option>
              {/each}
            </select>
            {#if convert}
              <span class="text-[11px] leading-snug text-faint">
                Tracks not already {FORMAT_NAMES[convert.format]} are converted on the USB. Your library isn’t changed.
              </span>
            {/if}
          </label>
          <button
            value="confirm"
            disabled={locked || pending || reviewing || reviewBlocked || reviewUnresolved || selectedIds.length === 0 || checked.size === 0}
            class="h-8 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
          >
            Sync
          </button>
          <label class="flex items-start gap-2 text-[12px] leading-snug text-muted">
            <input type="checkbox" class="mt-0.5" bind:checked={ejectAfter} disabled={locked || !canEject} />
            Eject after syncing
          </label>
          {#if locked || notice}
            <p class="flex items-start gap-2 text-[12px] leading-snug {locked ? 'text-muted' : 'text-ok'}" role="status" aria-atomic="true">
              {#if locked}
                <LoaderCircle class="mt-0.5 size-3.5 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />
              {:else}
                <Check class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
              {/if}
              <span>{locked ? phaseLabel() : notice}</span>
            </p>
          {/if}
        </div>
      </section>
    </div>

    </div>
  </form>
</dialog>
