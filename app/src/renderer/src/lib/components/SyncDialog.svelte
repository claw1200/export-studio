<script lang="ts">
  import {
    AlertTriangle,
    Check,
    ChevronRight,
    Folder,
    FolderOpen,
    Library,
    ListMusic,
    LoaderCircle,
    RefreshCw,
    Search,
    Sparkles,
    X
  } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { api } from '$lib/api'
  import { getApp } from '$lib/context'
  import { loadPresets } from '$lib/convertPresetStore'
  import { FORMAT_NAMES, presetKey, presetLabel } from '$lib/convertPresets'
  import { suspendModal } from '$lib/modal'
  import { buildTree, type TreeNode } from '$lib/library/tree'
  import { syncFraction } from '$lib/syncProgress'
  import { automaticResolutions, needsChoice } from '$lib/syncReview'
  import ProgressBar from './ProgressBar.svelte'
  import SyncChanges from './SyncChanges.svelte'
  import type { DeviceLibraryView, Volume } from '$lib/types'
  import type { SyncInspection, SyncPlan, SyncResolution } from '../../../../preload/engine'

  const COLLECTION_ID = 'collection'

  let {
    playlistId = '',
    volumeId: preferredVolume = '',
    onclose
  }: { playlistId?: string; volumeId?: string; onclose: () => void } = $props()

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
  let inspections = $state.raw<Record<string, SyncInspection>>({})
  let inspectionErrors = $state.raw<Record<string, string>>({})
  let resolutions = $state<Record<string, Record<string, SyncResolution>>>({})
  let reviewing = $state(false)
  let showStickContents = $state(false)
  let showErrorDetails = $state(false)
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
  const locked = $derived(library.syncing)
  const progressLabel = $derived(library.syncLabel)
  const progressFraction = $derived(syncFraction(library.syncProgress))
  const device = $derived(library.devices.find((item) => item.id === volumeId))
  const canEject = $derived(!!device?.ejectable)

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
  const reviewBlocked = $derived(
    selectedIds.some((id) => inspections[id]?.diagnostics.some((item) => item.severity === 'blocked'))
  )
  const reviewUnresolved = $derived(
    selectedIds.some((id) => {
      const inspection = inspections[id]
      return (
        !inspection ||
        inspection.diagnostics.some((item) => item.severity === 'decision_required' && !resolutions[id]?.[item.id])
      )
    })
  )

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
    void Promise.all(
      ids.map(async (id) => {
        try {
          return [id, await api.usbSyncReview(id, playlistIds, preset)] as const
        } catch (reason) {
          const message = reason instanceof Error ? reason.message : 'Could not inspect USB changes'
          return [id, message] as const
        }
      })
    )
      .then((results) => {
        if (cancelled || key !== reviewKey) return
        const nextInspections: Record<string, SyncInspection> = {}
        const nextErrors: Record<string, string> = {}
        const nextResolutions: Record<string, Record<string, SyncResolution>> = {}
        for (const [id, result] of results) {
          if (typeof result === 'string') nextErrors[id] = result
          else {
            nextInspections[id] = result
            nextResolutions[id] = automaticResolutions(result)
          }
        }
        inspections = nextInspections
        inspectionErrors = nextErrors
        resolutions = nextResolutions
      })
      .finally(() => {
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

  function pickDevice(id: string): void {
    picked = [id]
    confirm = null
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

  function planSummary(plan: SyncPlan): string {
    if (plan.upToDate) return 'The USB is already up to date'
    const parts = [`${plan.tracks} tracks in ${plan.playlists} playlists`]
    if (plan.copy) parts.push(`${plan.copy} to copy`)
    if (plan.convert) parts.push(`${plan.convert} to convert`)
    if (plan.remove) parts.push(`${plan.remove} to remove`)
    if (parts.length === 1) parts.push('no audio to copy')
    if (plan.estimatedPeakBytes != null) {
      parts.push(`${plan.approximate ? 'about ' : ''}${formatBytes(plan.estimatedPeakBytes)} needed`)
    }
    return parts.join(' · ')
  }

  /** The SDK's errors open with what happened, then list every file involved after a further colon. */
  const errorSummary = $derived(error?.split(': ').slice(0, 2).join(': ') ?? '')
  const inspection = $derived(inspections[volumeId])
  const inspectionError = $derived(inspectionErrors[volumeId])
  const readiness = $derived.by((): string => {
    if (!library.devices.length) return 'Connect a USB drive to sync'
    if (!volumeId) return 'Choose a USB'
    if (pending) return 'Reading the USB'
    if (checked.size === 0) return 'Select the playlists to sync'
    if (reviewing) return 'Checking what will change'
    if (inspectionError) return inspectionError
    if (reviewBlocked) return 'This USB can’t be synced until the issues listed are fixed'
    if (reviewUnresolved) return 'Choose an action for each change listed'
    return inspection?.plan ? planSummary(inspection.plan) : ''
  })
  const canSync = $derived(
    !locked && !pending && !reviewing && !reviewBlocked && !reviewUnresolved && !!volumeId && checked.size > 0
  )

  async function submit(): Promise<void> {
    const playlistIds = [...checked]
    if (!canSync) return
    error = null
    notice = null
    confirm = null
    const inspection = inspections[volumeId]
    const chosen = resolutions[volumeId] ?? {}
    const review =
      inspection && Object.keys(chosen).length ? { token: inspection.token, resolutions: { ...chosen } } : undefined
    const summary = await library.syncTo(volumeId, playlistIds, { review, convert, ejectAfter })
    deviceToken += 1
    refreshToken += 1
    if (summary) notice = summary
    else error = library.syncError
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
      {@const opened =
        isFolder &&
        ((term !== '' && (named || node.children.some((child) => nodeMatches(child, term)))) || open.has(key))}
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
  aria-label="Sync to USB"
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
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
      <h2 class="min-w-0 flex-1 text-[13px] font-semibold">Sync to USB</h2>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
        aria-label="Close"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="grid min-h-0 flex-1 grid-cols-2 divide-x divide-line-soft">
      <section class="flex min-h-0 min-w-0 flex-col p-5" aria-labelledby="sync-playlists">
        <div class="flex shrink-0 items-baseline justify-between gap-3">
          <h3 id="sync-playlists" class="text-[13px] font-semibold">Playlists</h3>
          <span class="text-[11.5px] text-muted">
            {pending ? 'Reading the USB' : `${stats.picked} of ${stats.total} selected`}
          </span>
        </div>
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
        <p class="mt-2 shrink-0 text-[11px] leading-relaxed text-faint">
          Unticked playlists from this library are removed from the USB. Playlists that aren’t in this library stay.
        </p>
      </section>

      <div
        class="flex min-h-0 min-w-0 flex-col gap-5 overflow-y-auto overscroll-contain p-5"
        {@attach loadLibraries(library.devices.map((item) => item.id).join('\n'), deviceToken)}
      >
        <section aria-labelledby="sync-device">
          <div class="flex items-center justify-between gap-3">
            <h3 id="sync-device" class="text-[13px] font-semibold">USB</h3>
            <button
              type="button"
              class="grid size-6 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
              title="Look for USB drives again"
              aria-label="Look for USB drives again"
              disabled={locked}
              onclick={() => {
                void library.refreshDevices()
                refreshToken += 1
                deviceToken += 1
              }}
            >
              <RefreshCw class="size-3.5" />
            </button>
          </div>

          {#if library.devices.length}
            <div class="mt-2 flex flex-col gap-1.5" role="radiogroup" aria-labelledby="sync-device">
              {#each library.devices as item (item.id)}
                {@const chosen = item.id === volumeId}
                {@const usage = share(item)}
                <div class={['rounded-md border', chosen ? 'border-accent/60 bg-accent/5' : 'border-line']}>
                  <label class="flex h-9 items-center gap-2 px-2.5 text-[12.5px]">
                    <input
                      type="radio"
                      name="sync-device"
                      class="shrink-0"
                      checked={chosen}
                      disabled={locked}
                      onchange={() => pickDevice(item.id)}
                    />
                    <span class="min-w-0 flex-1 truncate">{item.name}</span>
                    <span class="shrink-0 text-[11px] text-faint">
                      {[item.fileSystem, item.bytesTotal > 0 ? formatBytes(item.bytesTotal) : '']
                        .filter(Boolean)
                        .join(' · ')}
                    </span>
                  </label>
                  {#if chosen && item.bytesTotal > 0}
                    <div class="px-2.5 pb-2.5">
                      <div class="h-1 overflow-hidden rounded-full bg-line">
                        <div class="h-full bg-accent" style:width="{usage.used}%"></div>
                      </div>
                      <div class="mt-1.5 flex items-center justify-between gap-3 text-[11px] text-faint">
                        <span>{formatBytes(item.bytesFree)} free ({usage.free}%)</span>
                        {#if item.ejectable && confirm !== 'format'}
                          <button
                            type="button"
                            class="text-muted hover:text-danger disabled:opacity-40"
                            disabled={locked}
                            onclick={() => {
                              formatId = item.id
                              confirm = 'format'
                            }}
                          >
                            Format…
                          </button>
                        {/if}
                      </div>
                      {#if confirm === 'format' && formatId === item.id}
                        <div class="mt-2 rounded-md border border-danger/50 bg-danger/5 p-2.5">
                          <p class="text-[12px] leading-relaxed text-muted">
                            Erase everything on {item.name} and format it as FAT32? This can’t be undone.
                          </p>
                          <div class="mt-2 flex justify-end gap-2">
                            <button
                              type="button"
                              class="h-7 rounded-md px-3 text-[12px] text-muted hover:bg-hover hover:text-fg"
                              onclick={() => (confirm = null)}
                            >
                              Cancel
                            </button>
                            <button
                              type="button"
                              class="h-7 rounded-md bg-danger px-3 text-[12px] font-medium text-white hover:bg-danger/90"
                              onclick={() => void runFormat()}
                            >
                              Erase and format
                            </button>
                          </div>
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {:else}
            <p class="mt-2 rounded-md border border-dashed border-line px-3 py-4 text-center text-[12px] text-muted">
              Connect a USB drive.
            </p>
          {/if}

          {#if device}
            {@const libs = deviceLibraries[device.id]}
            <button
              type="button"
              class="mt-2 flex items-center gap-1 text-[11.5px] text-muted hover:text-fg"
              aria-expanded={showStickContents}
              onclick={() => (showStickContents = !showStickContents)}
            >
              <ChevronRight class={['size-3 transition-transform', showStickContents && 'rotate-90']} />
              Already on this USB
            </button>
            {#if showStickContents}
              <div
                class="mt-1.5 max-h-48 overflow-y-auto overscroll-contain rounded-md border border-line py-1"
                role="region"
                aria-label={`Playlists on ${device.name}`}
              >
                {#if libs === undefined}
                  <p class="px-2 py-1 text-[12px] text-faint">Reading this USB</p>
                {:else if libs.length === 0}
                  <p class="px-2 py-1 text-[12px] text-faint">Nothing on this USB yet</p>
                {:else}
                  {#each libs as lib (lib.id)}
                    {@const libraryKey = `${device.id}:${lib.id}`}
                    {@const libraryOpen = open.has(libraryKey)}
                    <button
                      type="button"
                      class="flex h-7 w-full items-center gap-1.5 px-2 text-left text-[12.5px] text-fg hover:bg-hover"
                      aria-expanded={libraryOpen}
                      onclick={() => toggleOpen(libraryKey)}
                    >
                      <ChevronRight
                        class={['size-3 shrink-0 text-faint transition-transform', libraryOpen && 'rotate-90']}
                      />
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
            {/if}
          {/if}
        </section>

        <section aria-labelledby="sync-options">
          <h3 id="sync-options" class="text-[13px] font-semibold">Options</h3>
          <label class="mt-2 flex flex-col gap-1 text-[12px] text-muted">
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
          <label class="mt-3 flex items-center gap-2 text-[12px] text-muted">
            <input type="checkbox" bind:checked={ejectAfter} disabled={locked || !canEject} />
            Eject after syncing
          </label>
        </section>

        {#if device && inspection?.diagnostics.length && !reviewing}
          {@const attention = reviewBlocked || inspection.diagnostics.some(needsChoice)}
          <section aria-labelledby="sync-review">
            <h3 id="sync-review" class="flex items-center gap-1.5 text-[13px] font-semibold">
              {#if attention}
                <AlertTriangle class="size-3.5 text-warn" />
                Needs your attention
              {:else}
                Changes on the USB
              {/if}
            </h3>
            <div class="mt-2">
              <SyncChanges
                deviceName={device.name}
                {inspection}
                resolutions={resolutions[device.id]}
                {locked}
                onresolve={(diagnostic, resolution) => chooseResolution(device.id, diagnostic, resolution)}
                onrecover={() => void recoverVolume(device.id)}
              />
            </div>
          </section>
        {/if}
      </div>
    </div>

    <footer class="flex min-h-14 shrink-0 items-center gap-4 border-t border-line-soft px-5 py-3">
      <div class="flex min-w-0 flex-1 flex-col gap-1.5" role="status" aria-atomic="true">
        {#if locked}
          <p class="flex items-center gap-2 text-[12px] text-muted">
            <LoaderCircle class="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            <span class="truncate">{progressLabel}</span>
          </p>
          {#if progressFraction !== null}
            <ProgressBar fraction={progressFraction} label={progressLabel} class="w-full" />
          {/if}
        {:else if error}
          <p class="line-clamp-2 text-[12px] leading-snug text-warn" title={errorSummary}>{errorSummary}</p>
          {#if errorSummary !== error}
            {#if showErrorDetails}
              <p
                class="max-h-32 overflow-y-auto overscroll-contain rounded-md border border-line bg-bg px-2 py-1.5 font-mono text-[10.5px] leading-relaxed break-words text-muted select-text"
              >
                {error}
              </p>
            {/if}
            <button
              type="button"
              class="self-start text-[11px] text-muted hover:text-fg"
              onclick={() => (showErrorDetails = !showErrorDetails)}
            >
              {showErrorDetails ? 'Hide details' : 'Show details'}
            </button>
          {/if}
        {:else if notice}
          <p class="flex items-center gap-2 text-[12px] text-ok">
            <Check class="size-3.5 shrink-0" aria-hidden="true" />
            {notice}
          </p>
        {:else}
          <p class={['flex items-center gap-2 text-[12px]', canSync ? 'text-fg' : 'text-muted']}>
            {#if reviewing || pending}
              <LoaderCircle class="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            {/if}
            <span class="truncate" title={readiness}>{readiness}</span>
          </p>
        {/if}
      </div>
      {#if library.syncingPlaylists}
        <button
          type="button"
          disabled={!library.canStopSync}
          title={library.canStopSync ? undefined : 'Finishing the sync so the USB stays complete'}
          class="h-8 shrink-0 rounded-md border border-line px-4 text-[12.5px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
          onclick={() => void library.stopSync()}
        >
          Stop sync
        </button>
      {:else}
        <button
          value="confirm"
          disabled={!canSync}
          class="h-8 shrink-0 rounded-md bg-accent px-5 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
        >
          Sync
        </button>
      {/if}
    </footer>
  </form>
</dialog>
