<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import {
    Library as LibraryIcon,
    ListMusic,
    Folder,
    FolderOpen,
    Sparkles,
    History,
    Tag,
    ChevronRight
  } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import type { TreeNode } from '$lib/library.svelte'
  import type { Source } from '$lib/types'

  let { onsource }: { onsource?: () => void } = $props()

  const { library } = getApp()

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

  function open(type: SourceType, node: TreeNode): void {
    if (node.kind === 'folder') toggle(`${type}:${node.id}`)
    else setSource({ type, id: node.id } as Source)
  }
</script>

{#snippet tree(nodes: TreeNode[], type: SourceType, depth: number)}
  {#each nodes as node (node.id)}
    {@const key = `${type}:${node.id}`}
    {@const isFolder = node.kind === 'folder'}
    {@const isOpen = expanded.has(key)}
    {@const active = !isFolder && library.isSource({ type, id: node.id } as Source)}
    <button
      class={[
        'group flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-[12.5px]',
        active ? 'bg-select text-fg' : 'text-muted hover:bg-hover hover:text-fg'
      ]}
      style:padding-left="{8 + depth * 14}px"
      onclick={() => open(type, node)}
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
      <span class="min-w-0 flex-1 truncate">{node.name || 'Untitled'}</span>
      {#if !isFolder}
        {@const count = type === 'tag' ? (library.tagCounts.get(node.id) ?? 0) : node.count}
        {#if count != null}
          <span class="tabular text-[11px] text-faint group-hover:text-muted">{count}</span>
        {/if}
      {/if}
    </button>
    {#if isFolder && isOpen}
      {@render tree(node.children, type, depth + 1)}
    {/if}
  {/each}
{/snippet}

{#snippet section(id: string, title: string, nodes: TreeNode[], type: SourceType)}
  {#if nodes.length}
    <div class="mt-4">
      <button
        class="flex w-full items-center gap-1 px-2 pb-1 text-[10.5px] font-semibold tracking-wider text-faint uppercase hover:text-muted"
        onclick={() => toggleSection(id)}
      >
        <ChevronRight class={['size-3 transition-transform', !collapsedSections.has(id) && 'rotate-90']} />
        {title}
      </button>
      {#if !collapsedSections.has(id)}
        {@render tree(nodes, type, 0)}
      {/if}
    </div>
  {/if}
{/snippet}

<nav class="flex h-full w-full flex-col overflow-y-auto border-r border-line-soft bg-bg px-2 py-3">
  <button
    class={[
      'flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] font-medium',
      library.source.type === 'collection' ? 'bg-select text-fg' : 'text-muted hover:bg-hover hover:text-fg'
    ]}
    onclick={() => setSource({ type: 'collection' })}
  >
    <LibraryIcon class="size-4 text-accent" />
    <span class="flex-1">Collection</span>
    <span class="tabular text-[11px] text-faint">{library.tracks.length}</span>
  </button>

  {@render section('playlists', 'Playlists', library.playlistTree, 'playlist')}
  {@render section('tags', 'My Tag', library.tagTree, 'tag')}
  {@render section('histories', 'Histories', library.historyTree, 'history')}
</nav>
