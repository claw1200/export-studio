<script lang="ts">
  import { Search } from '@lucide/svelte'
  import type { SyncDecisions, SyncReview } from '../../../../preload/engine'

  let {
    review,
    resolutions,
    locked,
    onchoose,
    onchooseall
  }: {
    review: SyncReview
    resolutions: SyncDecisions['resolutions']
    locked: boolean
    onchoose: (key: string, action: string) => void
    onchooseall: (action: 'restore' | 'keep') => void
  } = $props()

  let query = $state('')
  let confirmAction = $state<'restore' | 'keep' | null>(null)
  const unresolved = $derived(review.conflicts.filter((item) => !resolutions[item.key]).length)
  const rows = $derived(
    review.conflicts.filter((item) =>
      `${item.name} ${item.artist} ${item.path}`.toLowerCase().includes(query.trim().toLowerCase())
    )
  )
</script>

<section class="mb-5 border-b border-line pb-4" aria-label="Review USB changes">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <h2 class="text-[13.5px] font-semibold">Review USB changes</h2>
    {#if review.conflicts.length}
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="h-7 rounded-md border border-line px-2 text-[12px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
          disabled={locked}
          onclick={() => (confirmAction = 'restore')}
        >
          Restore all from library...
        </button>
        <button
          type="button"
          class="h-7 rounded-md border border-line px-2 text-[12px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
          disabled={locked}
          onclick={() => (confirmAction = 'keep')}
        >
          Keep all deleted...
        </button>
      </div>
    {/if}
  </div>
  {#if review.conflicts.length}
    <p class="mt-1 text-[12px] text-warn">
      {review.conflicts.length} deleted {review.conflicts.length === 1 ? 'item' : 'items'} &middot; {unresolved} unresolved
    </p>
    {#if confirmAction}
      <div class="mt-3 border-l-2 border-warn pl-3">
        <p class="text-[12px] text-muted">
          {#if confirmAction === 'restore'}
            Restore all {review.conflicts.length} deleted items to this USB from the library? This reverses their USB deletions.
          {:else}
            Keep all {review.conflicts.length} items deleted on this USB? Their exclusions will be saved for this device.
            Your computer library is not changed.
          {/if}
        </p>
        <div class="mt-2 flex gap-2">
          <button
            type="button"
            class="h-7 rounded-md bg-accent px-3 text-[12px] text-bg"
            disabled={locked}
            onclick={() => {
              if (confirmAction) onchooseall(confirmAction)
              confirmAction = null
            }}>{confirmAction === 'restore' ? 'Restore all' : 'Keep all deleted'}</button
          >
          <button
            type="button"
            class="h-7 rounded-md px-3 text-[12px] text-muted hover:bg-hover"
            disabled={locked}
            onclick={() => (confirmAction = null)}>Cancel</button
          >
        </div>
      </div>
    {/if}
    <label class="mt-3 flex h-8 items-center gap-2 rounded-md border border-line bg-bg px-2">
      <Search class="size-3.5 shrink-0 text-faint" />
      <input
        type="search"
        bind:value={query}
        placeholder="Search USB changes"
        aria-label="Search USB changes"
        class="min-w-0 flex-1 bg-transparent text-[12px] outline-none"
        onkeydown={(event) => {
          if (event.key === 'Enter') event.preventDefault()
        }}
      />
    </label>
    <div class="mt-2 max-h-56 overflow-auto">
      <table class="block w-full text-left text-[12px] sm:table">
        <thead class="sticky top-0 hidden bg-surface text-[11px] text-faint sm:table-header-group">
          <tr>
            <th class="py-2 pr-3 font-normal">Deleted Item</th>
            <th class="py-2 pr-3 font-normal">USB Library</th>
            <th class="py-2 font-normal">Action</th>
          </tr>
        </thead>
        <tbody class="block sm:table-row-group">
          {#each rows as item (item.key)}
            <tr class="block border-t border-line-soft sm:table-row">
              <td class="block py-2 pr-3 align-top sm:table-cell sm:max-w-56">
                <div class="break-words font-medium">{item.name || 'Untitled'}</div>
                <div class="break-words text-[11px] text-faint">
                  {item.entity === 'track' ? item.artist : item.path}
                </div>
                <div class="text-[11px] text-faint">
                  {item.entity === 'folder'
                    ? 'Folder and its playlists'
                    : item.entity === 'track'
                      ? 'Track in all USB playlists'
                      : 'Playlist only'}
                </div>
              </td>
              <td class="block pr-3 align-top text-[11px] text-muted sm:table-cell sm:py-2">{item.format}</td>
              <td class="block py-2 align-top sm:table-cell">
                <select
                  class="h-8 w-full max-w-full rounded-md border border-line bg-bg px-2 text-[12px] sm:w-auto"
                  aria-label={`Action for ${item.name}`}
                  disabled={locked}
                  value={resolutions[item.key] ?? ''}
                  onchange={(event) => onchoose(item.key, event.currentTarget.value)}
                >
                  <option value="">Choose action</option>
                  <option value="restore">Restore from library</option>
                  <option value="keep">Keep deleted on this USB</option>
                </select>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if !rows.length}<p class="py-2 text-[12px] text-faint">No matching changes</p>{/if}
    </div>
  {/if}
  {#if review.exclusions.length}
    <details class="mt-3 text-[12px] text-muted">
      <summary class="cursor-pointer">Excluded on this USB ({review.exclusions.length})</summary>
      {#each review.exclusions as item (item.key)}
        <label class="mt-2 flex items-start justify-between gap-3 border-t border-line-soft pt-2">
          <span class="min-w-0 break-words">{item.path || item.name}</span>
          <span class="flex shrink-0 items-center gap-2">
            <input
              type="checkbox"
              checked={resolutions[item.key] === 'restore'}
              disabled={locked}
              onchange={(event) => onchoose(item.key, event.currentTarget.checked ? 'restore' : '')}
            />Restore on sync
          </span>
        </label>
      {/each}
    </details>
  {/if}
</section>
