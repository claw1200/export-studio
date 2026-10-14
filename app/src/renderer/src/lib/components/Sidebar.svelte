<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import {
    Library as LibraryIcon,
    ListMusic,
    ListPlus,
    Folder,
    FolderOpen,
    FolderPlus,
    Sparkles,
    History,
    Tag,
    ChevronRight,
    Pencil,
    Plus,
    Trash2,
    HardDrive,
    FilePlus,
    FolderInput,
    FolderSearch,
    ListFilter,
    Copy,
    Eject,
    FolderSync,
    LoaderCircle
  } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { droppedPaths, isFileDrag } from '$lib/library/drop'
  import { buildTree, type TreeNode } from '$lib/library/tree'
  import { menuAt, tidy, type MenuEntry, type MenuRequest } from '$lib/menu'
  import { SEPARATOR, unlockEntry } from '$lib/trackMenu'
  import type { Source, View, Volume } from '$lib/types'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import ContextMenu from './ContextMenu.svelte'
  import SyncDialog from './SyncDialog.svelte'
  import ViewDialog from './ViewDialog.svelte'

  let { onsource }: { onsource?: () => void } = $props()

  const { library, player, audition } = getApp()

  let ejectingId = $state<string | null>(null)

  function trackOnVolume(trackId: string | null | undefined, volumeId: string): boolean {
    if (!trackId?.startsWith('usb:')) return false
    const body = trackId.slice(4)
    const split = body.lastIndexOf(':')
    if (split <= 0) return false
    try {
      return decodeURIComponent(body.slice(0, split)) === volumeId
    } catch {
      return false
    }
  }

  async function ejectVolume(volume: Volume): Promise<void> {
    if (library.syncing || ejectingId) return
    ejectingId = volume.id
    if (trackOnVolume(audition.trackId, volume.id)) audition.stop()
    if (trackOnVolume(player.track?.id, volume.id)) player.eject()
    try {
      await library.ejectDevice(volume.id)
    } catch (e) {
      library.notify((e as Error).message)
    } finally {
      if (ejectingId === volume.id) ejectingId = null
    }
  }

  function setSource(source: Source): void {
    library.setSource(source)
    onsource?.()
  }

  const expanded = new SvelteSet<string>()
  const collapsedSections = new SvelteSet<string>(['histories'])

  type SourceType = 'playlist' | 'history' | 'tag'

  function toggle(id: string): void {
    if (expanded.has(id)) expanded.delete(id)
    else expanded.add(id)
  }

  function toggleSection(id: string): void {
    if (collapsedSections.has(id)) collapsedSections.delete(id)
    else collapsedSections.add(id)
  }

  function sectionOpen(id: string): boolean {
    return !collapsedSections.has(id) || (id === 'playlists' && !!library.renamingId)
  }

  function open(type: SourceType, node: TreeNode): void {
    if (node.kind === 'folder') toggle(`${type}:${node.id}`)
    else setSource({ type, id: node.id } as Source)
  }

  const EXPAND_DELAY = 600
  let dropKey = $state<string | null>(null)
  let expandKey: string | null = null
  let expandTimer: ReturnType<typeof setTimeout> | undefined

  function accepts(type: SourceType, node: TreeNode): boolean {
    return (type === 'playlist' && node.kind === 'playlist') || (type === 'tag' && node.kind === 'tag')
  }

  function expandSoon(key: string, expand: () => void): void {
    if (expandKey === key) return
    clearTimeout(expandTimer)
    expandKey = key
    expandTimer = setTimeout(() => {
      expandKey = null
      expand()
    }, EXPAND_DELAY)
  }

  function cancelExpand(key: string): void {
    if (expandKey !== key) return
    clearTimeout(expandTimer)
    expandKey = null
  }

  function dragOver(e: DragEvent, type: SourceType, node: TreeNode): void {
    const files = !library.dragging && isFileDrag(e)
    if (!library.dragging && !files) return
    const key = `${type}:${node.id}`
    if (node.kind === 'folder' && !expanded.has(key)) expandSoon(key, () => expanded.add(key))
    if (files ? type !== 'playlist' || node.kind !== 'playlist' : !accepts(type, node)) return
    if (!library.writable) {
      library.notify(
        files
          ? 'Unlock editing to import tracks'
          : `Unlock editing to add tracks to ${type === 'tag' ? 'tags' : 'playlists'}`
      )
      return
    }
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
    dropKey = key
  }

  function collectionDragOver(e: DragEvent): void {
    if (library.dragging || !isFileDrag(e)) return
    if (!library.writable) {
      library.notify('Unlock editing to import tracks')
      return
    }
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
    dropKey = 'collection'
  }

  function collectionDrop(e: DragEvent): void {
    dropKey = null
    if (library.dragging || !isFileDrag(e) || !library.writable) return
    e.preventDefault()
    void library.importFiles(droppedPaths(e))
  }

  function importItems(): MenuEntry[] {
    const locked = !library.writable
    return [
      { label: 'Import tracks…', icon: FilePlus, disabled: locked, onselect: () => void library.chooseImport('files') },
      {
        label: 'Import folder…',
        icon: FolderInput,
        disabled: locked,
        onselect: () => void library.chooseImport('folder')
      }
    ]
  }

  function collectionMenu(e: MouseEvent): void {
    e.preventDefault()
    menu = menuAt(
      e,
      'Collection',
      tidy([...importItems(), !library.writable && SEPARATOR, !library.writable && unlockEntry(library, player)])
    )
  }

  function dragLeave(e: DragEvent, key: string): void {
    const into = e.relatedTarget
    if (into instanceof Node && (e.currentTarget as HTMLElement).contains(into)) return
    if (dropKey === key) dropKey = null
    cancelExpand(key)
  }

  function drop(e: DragEvent, type: SourceType, node: TreeNode): void {
    dropKey = null
    if (!library.dragging && isFileDrag(e)) {
      if (type !== 'playlist' || node.kind !== 'playlist' || !library.writable) return
      e.preventDefault()
      void library.importFiles(droppedPaths(e), node.id)
      return
    }
    const ids = library.dragging
    if (!ids || !library.writable || !accepts(type, node)) return
    e.preventDefault()
    if (type === 'playlist') void library.addToPlaylist(node.id, ids)
    else void library.setTag(node.id, ids, true)
  }

  let menu = $state<MenuRequest | null>(null)
  let deleting = $state<TreeNode | null>(null)
  let syncing = $state<{ playlistId: string; volumeId: string } | null>(null)
  let viewDialog = $state<View | 'new' | null>(null)
  let deletingView = $state<View | null>(null)

  function viewsMenu(e: MouseEvent): void {
    e.preventDefault()
    menu = menuAt(e, 'Views', [{ label: 'New view', icon: Plus, onselect: () => (viewDialog = 'new') }])
  }

  function viewMenu(e: MouseEvent, view: View): void {
    e.preventDefault()
    menu = menuAt(e, 'View', [
      { label: 'Edit', icon: Pencil, onselect: () => (viewDialog = view) },
      { label: 'Delete', icon: Trash2, danger: true, onselect: () => (deletingView = view) }
    ])
  }

  function create(parentId: string, folder: boolean): void {
    if (parentId !== 'root') expanded.add(`playlist:${parentId}`)
    collapsedSections.delete('playlists')
    void library.createPlaylist(parentId, folder)
  }

  function createItems(parentId: string): MenuEntry[] {
    const locked = !library.writable
    return [
      { label: 'New playlist', icon: ListPlus, disabled: locked, onselect: () => create(parentId, false) },
      { label: 'New folder', icon: FolderPlus, disabled: locked, onselect: () => create(parentId, true) }
    ]
  }

  function sectionMenu(e: MouseEvent): void {
    e.preventDefault()
    menu = menuAt(
      e,
      'Playlists',
      tidy([...createItems('root'), !library.writable && SEPARATOR, !library.writable && unlockEntry(library, player)])
    )
  }

  function playlistMenu(e: MouseEvent, node: TreeNode): void {
    e.preventDefault()
    const locked = !library.writable
    const isFolder = node.kind === 'folder'
    menu = menuAt(
      e,
      isFolder ? 'Folder' : 'Playlist',
      tidy([
        ...(isFolder ? [...createItems(node.id), SEPARATOR] : []),
        {
          label: 'Sync to…',
          icon: FolderSync,
          onselect: () => openSync(node.id, '')
        },
        { label: 'Rename', icon: Pencil, disabled: locked, onselect: () => (library.renamingId = node.id) },
        { label: 'Delete', icon: Trash2, danger: true, disabled: locked, onselect: () => (deleting = node) },
        locked && SEPARATOR,
        locked && unlockEntry(library, player)
      ])
    )
  }

  function deleteMessage(node: TreeNode): string {
    const name = node.name || 'Untitled'
    if (node.kind === 'folder') {
      return `Delete the folder “${name}” and every playlist inside it? The tracks stay in your collection.`
    }
    return `Delete the playlist “${name}”? The tracks stay in your collection.`
  }

  function startRename(input: HTMLInputElement): void {
    input.focus()
    input.select()
    input.scrollIntoView({ block: 'nearest' })
  }

  function commitRename(node: TreeNode, value: string): void {
    if (library.renamingId === node.id) void library.renamePlaylist(node.id, value)
  }

  function renameKey(e: KeyboardEvent, node: TreeNode): void {
    e.stopPropagation()
    if (e.key === 'Enter') {
      e.preventDefault()
      commitRename(node, e.currentTarget instanceof HTMLInputElement ? e.currentTarget.value : node.name)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      library.renamingId = null
    }
  }
  function openSync(playlistId: string, volumeId: string): void {
    void library.refreshDevices()
    syncing = { playlistId, volumeId }
  }

  async function toggleDevice(volumeId: string, hasLibrary: boolean): Promise<void> {
    const key = `device:${volumeId}`
    if (expanded.has(key)) {
      expanded.delete(key)
      return
    }
    if (hasLibrary) {
      try {
        await library.devicePlaylistTree(volumeId)
      } catch (e) {
        library.notify((e as Error).message)
        return
      }
    }
    expanded.add(key)
  }
</script>

{#snippet deviceTree(nodes: TreeNode[], volumeId: string, depth: number)}
  {#each nodes as node (node.id)}
    {@const key = `device-playlist:${volumeId}:${node.id}`}
    {@const isFolder = node.kind === 'folder'}
    {@const isOpen = expanded.has(key)}
    {@const active = !isFolder && library.isSource({ type: 'device', volumeId, playlistId: node.id })}
    <button
      class={[
        'flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-[12.5px]',
        active ? 'bg-select text-fg' : 'text-muted hover:bg-hover hover:text-fg'
      ]}
      style:padding-left="{8 + depth * 14}px"
      onclick={() => (isFolder ? toggle(key) : setSource({ type: 'device', volumeId, playlistId: node.id }))}
    >
      {#if isFolder}
        <ChevronRight class={['size-3 shrink-0 text-faint transition-transform', isOpen && 'rotate-90']} />
        {#if isOpen}
          <FolderOpen class="size-3.5 shrink-0 text-faint" />
        {:else}
          <Folder class="size-3.5 shrink-0 text-faint" />
        {/if}
      {:else}
        <span class="w-3 shrink-0"></span>
        <ListMusic class="size-3.5 shrink-0 text-faint" />
      {/if}
      <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
      {#if !isFolder && node.count != null}
        <span class="tabular text-[11px] text-faint">{node.count}</span>
      {/if}
    </button>
    {#if isFolder && isOpen}
      {@render deviceTree(node.children, volumeId, depth + 1)}
    {/if}
  {/each}
{/snippet}

{#snippet icon(node: TreeNode, type: SourceType, isOpen: boolean)}
  {#if node.kind === 'folder'}
    <ChevronRight class={['size-3 shrink-0 text-faint transition-transform', isOpen && 'rotate-90']} />
    {#if isOpen}
      <FolderOpen class="size-3.5 shrink-0 text-faint" />
    {:else}
      <Folder class="size-3.5 shrink-0 text-faint" />
    {/if}
  {:else}
    <span class="w-3 shrink-0"></span>
    {#if node.kind === 'smart'}
      <Sparkles class="size-3.5 shrink-0 text-accent/80" />
    {:else if type === 'history'}
      <History class="size-3.5 shrink-0 text-faint" />
    {:else if type === 'tag'}
      <Tag class="size-3.5 shrink-0 text-faint" />
    {:else}
      <ListMusic class="size-3.5 shrink-0 text-faint" />
    {/if}
  {/if}
{/snippet}

{#snippet tree(nodes: TreeNode[], type: SourceType, depth: number)}
  {#each nodes as node (node.id)}
    {@const key = `${type}:${node.id}`}
    {@const isFolder = node.kind === 'folder'}
    {@const isOpen = expanded.has(key)}
    {@const active = !isFolder && library.isSource({ type, id: node.id } as Source)}
    {#if type === 'playlist' && library.renamingId === node.id}
      <div
        class="flex h-7 w-full items-center gap-1.5 rounded-md bg-hover pr-1"
        style:padding-left="{8 + depth * 14}px"
      >
        {@render icon(node, type, isOpen)}
        <input
          {@attach startRename}
          value={node.name}
          spellcheck="false"
          aria-label="Playlist name"
          class="h-5.5 min-w-0 flex-1 rounded border border-accent/60 bg-bg px-1.5 text-[12.5px] text-fg"
          onkeydown={(e) => renameKey(e, node)}
          onblur={(e) => commitRename(node, e.currentTarget.value)}
        />
      </div>
    {:else}
      <button
        class={[
          'group flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-[12.5px]',
          dropKey === key
            ? 'bg-accent-soft text-fg ring-1 ring-accent/70 ring-inset'
            : active
              ? 'bg-select text-fg'
              : 'text-muted hover:bg-hover hover:text-fg'
        ]}
        style:padding-left="{8 + depth * 14}px"
        onclick={() => open(type, node)}
        oncontextmenu={type === 'playlist' ? (e) => playlistMenu(e, node) : undefined}
        ondragover={(e) => dragOver(e, type, node)}
        ondragleave={(e) => dragLeave(e, key)}
        ondrop={(e) => drop(e, type, node)}
      >
        {@render icon(node, type, isOpen)}
        <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
        {#if !isFolder}
          {@const count = type === 'tag' ? (library.tagCounts.get(node.id) ?? 0) : node.count}
          {#if count != null}
            <span class="tabular text-[11px] text-faint group-hover:text-muted">{count}</span>
          {/if}
        {/if}
      </button>
    {/if}
    {#if isFolder && isOpen}
      {@render tree(node.children, type, depth + 1)}
    {/if}
  {/each}
{/snippet}

{#snippet section(id: string, title: string, nodes: TreeNode[], type: SourceType)}
  {#if nodes.length || type === 'playlist'}
    {@const sectionKey = `section:${id}`}
    <div class="mt-4">
      <div
        class="group/section flex items-center pr-1"
        role="presentation"
        oncontextmenu={type === 'playlist' ? sectionMenu : undefined}
      >
        <button
          class="flex flex-1 items-center gap-1 px-2 pb-1 text-[10.5px] font-semibold tracking-wider text-faint uppercase hover:text-muted"
          onclick={() => toggleSection(id)}
          ondragover={(e) => {
            if ((library.dragging || isFileDrag(e)) && collapsedSections.has(id)) {
              expandSoon(sectionKey, () => collapsedSections.delete(id))
            }
          }}
          ondragleave={(e) => dragLeave(e, sectionKey)}
        >
          <ChevronRight class={['size-3 transition-transform', sectionOpen(id) && 'rotate-90']} />
          {title}
        </button>
        {#if type === 'playlist'}
          <button
            class="-mt-1 grid size-5 place-items-center rounded text-faint opacity-0 group-hover/section:opacity-100 hover:bg-hover hover:text-fg focus-visible:opacity-100"
            aria-label="New playlist or folder"
            title="New playlist or folder"
            onclick={sectionMenu}
          >
            <Plus class="size-3.5" />
          </button>
        {/if}
      </div>
      {#if sectionOpen(id)}
        {@render tree(nodes, type, 0)}
        {#if !nodes.length}
          <p class="px-2 py-1 text-[12px] text-faint">No playlists yet</p>
        {/if}
      {/if}
    </div>
  {/if}
{/snippet}

<nav class="flex h-full w-full flex-col overflow-y-auto border-r border-line-soft bg-bg px-2 py-3">
  <button
    class={[
      'flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] font-medium',
      dropKey === 'collection'
        ? 'bg-accent-soft text-fg ring-1 ring-accent/70 ring-inset'
        : library.source.type === 'collection'
          ? 'bg-select text-fg'
          : 'text-muted hover:bg-hover hover:text-fg'
    ]}
    onclick={() => setSource({ type: 'collection' })}
    oncontextmenu={collectionMenu}
    ondragover={collectionDragOver}
    ondragleave={(e) => dragLeave(e, 'collection')}
    ondrop={collectionDrop}
  >
    <LibraryIcon class="size-4 text-accent" />
    <span class="flex-1">Collection</span>
    <span class="tabular text-[11px] text-faint">{library.collectionSize}</span>
  </button>

  <div class="mt-4">
    <button
      class="flex w-full items-center gap-1 px-2 pb-1 text-[10.5px] font-semibold tracking-wider text-faint uppercase hover:text-muted"
      onclick={() => toggleSection('devices')}
    >
      <ChevronRight class={['size-3 transition-transform', sectionOpen('devices') && 'rotate-90']} />
      Devices
    </button>
    {#if sectionOpen('devices')}
      {#each library.devices as volume (volume.id)}
        {@const key = `device:${volume.id}`}
        {@const open = expanded.has(key)}
        {@const active = library.isSource({ type: 'device', volumeId: volume.id, playlistId: null })}
        <div class="flex h-7 items-center gap-1 rounded-md pr-2" class:bg-select={active && volume.hasLibrary}>
          <button
            class="grid size-7 place-items-center text-faint hover:text-fg"
            aria-label={open ? `Collapse ${volume.name}` : `Expand ${volume.name}`}
            onclick={() => void toggleDevice(volume.id, volume.hasLibrary)}
          >
            <ChevronRight class={['size-3 transition-transform', open && 'rotate-90']} />
          </button>
          <button
            class={[
              'flex h-7 min-w-0 flex-1 items-center gap-1.5 text-left text-[12.5px]',
              active && volume.hasLibrary ? 'text-fg' : 'text-muted hover:text-fg'
            ]}
            disabled={!volume.hasLibrary}
            onclick={() => volume.hasLibrary && setSource({ type: 'device', volumeId: volume.id, playlistId: null })}
          >
            <HardDrive class="size-3.5 shrink-0 text-faint" />
            <span class="min-w-0 flex-1 truncate">{volume.name}</span>
            {#if !volume.hasLibrary}
              <span class="text-[11px] text-faint">Empty</span>
            {/if}
          </button>
          <button
            class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
            aria-label="Sync {volume.name}"
            title={library.syncing ? 'Wait for the sync to finish' : `Sync ${volume.name}`}
            disabled={library.syncing}
            onclick={() => openSync('', volume.id)}
          >
            <FolderSync class="size-3.5" />
          </button>
          {#if volume.ejectable}
            <button
              class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
              aria-label="Eject {volume.name}"
              title={library.syncing ? 'Wait for the sync to finish' : `Eject ${volume.name}`}
              disabled={library.syncing || ejectingId === volume.id}
              onclick={() => void ejectVolume(volume)}
            >
              {#if ejectingId === volume.id}
                <LoaderCircle class="size-3.5 animate-spin" />
              {:else}
                <Eject class="size-3.5" />
              {/if}
            </button>
          {/if}
        </div>
        {#if open && volume.hasLibrary}
          {@render deviceTree(buildTree(library.devicePlaylists[volume.id] ?? []), volume.id, 1)}
        {/if}
      {:else}
        <p class="px-2 py-1 text-[12px] text-faint">No devices</p>
      {/each}
    {/if}
  </div>

  <div class="mt-4">
    <div class="group/section flex items-center pr-1" role="presentation" oncontextmenu={viewsMenu}>
      <button
        class="flex flex-1 items-center gap-1 px-2 pb-1 text-[10.5px] font-semibold tracking-wider text-faint uppercase hover:text-muted"
        onclick={() => toggleSection('views')}
      >
        <ChevronRight class={['size-3 transition-transform', sectionOpen('views') && 'rotate-90']} />
        Views
      </button>
      <button
        class="-mt-1 grid size-5 place-items-center rounded text-faint opacity-0 group-hover/section:opacity-100 hover:bg-hover hover:text-fg focus-visible:opacity-100"
        aria-label="New view"
        title="New view"
        onclick={() => (viewDialog = 'new')}
      >
        <Plus class="size-3.5" />
      </button>
    </div>
    {#if sectionOpen('views')}
      {#each library.views as view (view.id)}
        {@const active = library.isSource({ type: 'view', id: view.id })}
        <button
          class={[
            'flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12.5px]',
            active ? 'bg-select text-fg' : 'text-muted hover:bg-hover hover:text-fg'
          ]}
          onclick={() => setSource({ type: 'view', id: view.id })}
          oncontextmenu={(e) => viewMenu(e, view)}
        >
          {#if view.type === 'files'}
            <FolderSearch class="size-3.5 shrink-0 text-faint" />
          {:else if view.type === 'duplicates'}
            <Copy class="size-3.5 shrink-0 text-faint" />
          {:else}
            <ListFilter class="size-3.5 shrink-0 text-faint" />
          {/if}
          <span class="min-w-0 flex-1 truncate">{view.name || 'Untitled'}</span>
        </button>
      {:else}
        <p class="px-2 py-1 text-[12px] text-faint">No views yet</p>
      {/each}
    {/if}
  </div>

  {@render section('playlists', 'Playlists', library.playlistTree, 'playlist')}
  {@render section('tags', 'My Tag', library.tagTree, 'tag')}
  {@render section('histories', 'Histories', library.historyTree, 'history')}
</nav>

{#if menu}
  <ContextMenu {menu} onclose={() => (menu = null)} />
{/if}

{#if deleting}
  {@const node = deleting}
  <ConfirmDialog
    title={node.kind === 'folder' ? 'Delete folder' : 'Delete playlist'}
    message={deleteMessage(node)}
    confirmLabel="Delete"
    danger
    onconfirm={() => void library.deletePlaylist(node.id)}
    onclose={() => (deleting = null)}
  />
{/if}

{#if syncing}
  {@const target = syncing}
  <SyncDialog playlistId={target.playlistId} volumeId={target.volumeId} onclose={() => (syncing = null)} />
{/if}

{#if viewDialog}
  <ViewDialog view={viewDialog === 'new' ? null : viewDialog} onclose={() => (viewDialog = null)} />
{/if}

{#if deletingView}
  {@const view = deletingView}
  <ConfirmDialog
    title="Delete view"
    message={`Delete the view “${view.name || 'Untitled'}”?`}
    confirmLabel="Delete"
    danger
    onconfirm={() => library.deleteView(view.id)}
    onclose={() => (deletingView = null)}
  />
{/if}
