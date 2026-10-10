<script lang="ts">
  import { Music, FolderSearch, Repeat, AlertTriangle, X, Pencil, Star } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { api, artworkUrl } from '$lib/api'
  import {
    cueColor,
    formatBpm,
    formatClock,
    formatDuration,
    formatSize,
    hotCueLetter,
    keyColor,
    TRACK_COLORS
  } from '$lib/format'
  import { loadDetail } from '$lib/deck/session.svelte'
  import type { Cue, EditableField, Track, TrackDetail } from '$lib/types'
  import EditableText from './EditableText.svelte'

  let { onjump, onedit }: { onjump: (seconds: number) => void; onedit: (tracks: Track[]) => void } = $props()

  const { library, player } = getApp()
  const track = $derived(library.selected)
  const loose = $derived(track?.collected === false)
  const editable = $derived(library.writable && !loose)
  const canEditSelection = $derived(library.writable && library.selectedTracks.some((item) => item.collected !== false))
  const MAX_NUMBER = 9999

  const detailPromise = $derived(track && !loose ? loadDetail(track.id).then((d) => d.detail) : null)

  type Row = { label: string; value: string; field?: EditableField; numeric?: boolean }

  const fields = $derived.by((): Row[] => {
    if (!track) return []
    const rows: Row[] = [
      { label: 'Album', value: track.album, field: 'album' },
      { label: 'Genre', value: track.genre, field: 'genre' },
      { label: 'Label', value: track.label, field: 'label' },
      { label: 'Remixer', value: track.remixer, field: 'remixer' },
      { label: 'Original artist', value: track.originalArtist, field: 'originalArtist' },
      { label: 'Composer', value: track.composer, field: 'composer' },
      { label: 'Year', value: track.year ? String(track.year) : '', field: 'year', numeric: true },
      { label: 'Track no.', value: track.trackNo ? String(track.trackNo) : '', field: 'trackNo', numeric: true },
      { label: 'Date added', value: track.dateAdded },
      { label: 'Play count', value: track.playCount ? String(track.playCount) : '' },
      {
        label: 'Format',
        value: [
          track.fileType,
          track.sampleRate ? `${(track.sampleRate / 1000).toFixed(1)} kHz` : '',
          track.bitDepth ? `${track.bitDepth}-bit` : '',
          track.bitrate ? `${track.bitrate} kbps` : ''
        ]
          .filter(Boolean)
          .join(' · ')
      },
      { label: 'Size', value: formatSize(track.fileSize) }
    ]
    return rows.filter((row) => row.value || (editable && row.field))
  })

  function saveField(id: string, field: EditableField, label: string, numeric = false) {
    return async (value: string): Promise<boolean> => {
      if (!numeric) return library.updateTracks([id], { [field]: value })
      const n = value ? Number(value) : 0
      if (!Number.isInteger(n) || n < 0 || n > MAX_NUMBER) {
        library.notify(`${label} must be a whole number between 0 and ${MAX_NUMBER}`)
        return false
      }
      return library.updateTracks([id], { [field]: n })
    }
  }

  function setRating(t: Track, n: number): void {
    void library.updateTracks([t.id], { rating: t.rating === n ? 0 : n })
  }

  const tags = $derived(track ? track.myTags.map((id) => library.tagNames.get(id)).filter(Boolean) : [])
  const canRemoveCues = $derived(library.writable && !!player.detail && player.track?.id === track?.id)

  function removeCueLabel(cue: Cue): string {
    if (cue.kind === 'hot') return `Remove hot cue ${hotCueLetter(cue.slot)}`
    return `Remove memory cue at ${formatClock(cue.start)}`
  }
</script>

<aside class="flex h-full w-full flex-col overflow-y-auto border-l border-line-soft bg-bg">
  {#if library.selectedTracks.length > 1}
    <div class="flex items-center gap-2 border-b border-line-soft bg-surface px-4 py-2 text-[12px]">
      <span class="min-w-0 flex-1 truncate text-muted">{library.selectedTracks.length} tracks selected</span>
      <button
        class="flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 font-medium text-accent hover:bg-accent-soft disabled:pointer-events-none disabled:opacity-40"
        disabled={!canEditSelection}
        title={canEditSelection ? undefined : 'Unlock editing to change tracks'}
        onclick={() => onedit(library.selectedTracks.filter((item) => item.collected !== false))}
      >
        <Pencil class="size-3" />
        Edit all
      </button>
    </div>
  {/if}
  {#if track}
    <div class="p-4">
      <div class="aspect-square w-full overflow-hidden rounded-lg bg-raised shadow-xl shadow-black/40">
        {#if track.hasArtwork}
          <img src={artworkUrl(track.id, '')} alt="" class="size-full object-cover" draggable="false" />
        {:else}
          <div class="grid size-full place-items-center text-faint"><Music class="size-10" /></div>
        {/if}
      </div>
      <div class="mt-4 flex items-start gap-2">
        <div class="min-w-0 flex-1">
          <h2 class="text-[16px] leading-snug font-semibold text-fg">
            <EditableText value={track.title} label="Title" {editable} onsave={saveField(track.id, 'title', 'Title')} />
          </h2>
          <div class="mt-0.5 text-[13px] text-muted">
            <EditableText
              value={track.artist}
              label="Artist"
              placeholder="Unknown artist"
              {editable}
              onsave={saveField(track.id, 'artist', 'Artist')}
            />
            {#if !editable && !track.artist}<span>Unknown artist</span>{/if}
          </div>
        </div>
        {#if editable}
          {@const current = track}
          <button
            class="mt-0.5 grid size-6 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg"
            onclick={() => onedit([current])}
            aria-label="Edit track info"
            title="Edit track info"
          >
            <Pencil class="size-3.5" />
          </button>
        {/if}
      </div>

      <div class="tabular mt-3 grid grid-cols-3 gap-1.5 text-center">
        <div class="rounded-md bg-surface py-2">
          <div class="text-[15px] font-semibold text-fg">{formatBpm(track.bpm) || '–'}</div>
          <div class="text-[10px] tracking-wider text-faint uppercase">BPM</div>
        </div>
        <div class="rounded-md bg-surface py-2">
          <div class="text-[15px] font-semibold" style:color={keyColor(track.key) ?? 'inherit'}>{track.key || '–'}</div>
          <div class="text-[10px] tracking-wider text-faint uppercase">Key</div>
        </div>
        <div class="rounded-md bg-surface py-2">
          <div class="text-[15px] font-semibold text-fg">{formatDuration(track.duration)}</div>
          <div class="text-[10px] tracking-wider text-faint uppercase">Time</div>
        </div>
      </div>
    </div>

    <dl
      class="grid grid-cols-[auto_1fr] items-center gap-x-4 gap-y-1.5 border-t border-line-soft px-4 py-3 text-[12px]"
    >
      {#each fields as row (row.label)}
        <dt class="text-faint">{row.label}</dt>
        <dd class="flex min-w-0 justify-end text-right text-muted" title={row.value}>
          {#if row.field}
            <EditableText
              value={row.value}
              label={row.label}
              placeholder="Add"
              numeric={row.numeric}
              {editable}
              class="text-right"
              onsave={saveField(track.id, row.field, row.label, row.numeric)}
            />
          {:else}
            <span class="truncate select-text">{row.value}</span>
          {/if}
        </dd>
      {/each}
      {#if editable || track.rating}
        {@const current = track}
        <dt class="text-faint">Rating</dt>
        <dd class="flex justify-end">
          <div class="flex gap-px" role={editable ? 'radiogroup' : 'img'} aria-label="Rating {current.rating} of 5">
            {#each [1, 2, 3, 4, 5] as n (n)}
              {@const lit = n <= current.rating}
              {#if editable}
                <button
                  class="grid size-4.5 place-items-center rounded hover:bg-hover"
                  role="radio"
                  aria-checked={current.rating === n}
                  aria-label="{n} star{n === 1 ? '' : 's'}"
                  onclick={() => setRating(current, n)}
                >
                  <Star class={['size-3', lit ? 'fill-warn text-warn' : 'text-line']} strokeWidth={lit ? 0 : 2} />
                </button>
              {:else}
                <Star class={['size-3', lit ? 'fill-warn text-warn' : 'text-line']} strokeWidth={lit ? 0 : 2} />
              {/if}
            {/each}
          </div>
        </dd>
      {/if}
      {#if editable}
        {@const current = track}
        <dt class="text-faint">Colour</dt>
        <dd class="flex justify-end">
          <div class="flex gap-1" role="radiogroup" aria-label="Colour">
            <button
              class={[
                'grid size-3.5 place-items-center rounded-full border text-faint',
                current.color === 0 ? 'border-fg' : 'border-line hover:border-muted'
              ]}
              role="radio"
              aria-checked={current.color === 0}
              aria-label="No colour"
              title="No colour"
              onclick={() => void library.updateTracks([current.id], { color: 0 })}
            >
              <X class="size-2.5" />
            </button>
            {#each Object.entries(TRACK_COLORS) as [n, c] (n)}
              <button
                class={[
                  'size-3.5 rounded-full ring-offset-1 ring-offset-bg',
                  current.color === Number(n) ? 'ring-[1.5px] ring-fg' : 'hover:ring-1 hover:ring-muted'
                ]}
                style:background={c.hex}
                role="radio"
                aria-checked={current.color === Number(n)}
                aria-label={c.name}
                title={c.name}
                onclick={() => void library.updateTracks([current.id], { color: Number(n) })}
              ></button>
            {/each}
          </div>
        </dd>
      {:else if TRACK_COLORS[track.color]}
        <dt class="text-faint">Colour</dt>
        <dd class="truncate text-right text-muted">{TRACK_COLORS[track.color].name}</dd>
      {/if}
    </dl>

    {#if tags.length}
      <div class="flex flex-wrap gap-1 border-t border-line-soft px-4 py-3">
        {#each tags as tag (tag)}
          <span class="rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-medium text-accent">{tag}</span>
        {/each}
      </div>
    {/if}

    {#if track.comment || editable}
      <div class="border-t border-line-soft px-4 py-3 text-[12px] text-muted">
        <EditableText
          value={track.comment}
          label="Comment"
          placeholder="Add a comment"
          multiline
          {editable}
          onsave={saveField(track.id, 'comment', 'Comment')}
        />
      </div>
    {/if}

    {#if loose}
      {@render file()}
    {:else}
      {#await detailPromise}
        {@render cues(null)}
        {@render file()}
      {:then detail}
        {@render cues(player.detail?.id === detail?.id ? player.detail : detail)}
        {@render file()}
      {:catch}
        {@render file()}
      {/await}
    {/if}
  {:else}
    <div class="grid h-full place-items-center p-6 text-center text-[12px] text-faint">
      Select a track to see its details
    </div>
  {/if}
</aside>

{#snippet cues(detail: TrackDetail | null)}
  {#if track}
    <div class="border-t border-line-soft px-4 py-3">
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-[10.5px] font-semibold tracking-wider text-faint uppercase">Cues</h3>
        {#if detail}
          <span class="text-[11px] text-faint">{detail.cues.length}</span>
        {/if}
      </div>
      {#if detail?.cues.length}
        <ul class="tabular space-y-0.5">
          {#each detail.cues as cue (cue.id)}
            <li
              class="flex items-center rounded-md hover:bg-surface"
              onpointerenter={() => {
                if (player.track?.id === track.id) player.setPreview(cue.id, cue.start / 1000)
              }}
              onpointerleave={() => player.clearPreview(cue.id)}
            >
              <button
                class="flex min-w-0 flex-1 items-center gap-2 px-1.5 py-1 text-left text-[12px] disabled:cursor-default"
                disabled={player.track?.id !== track.id}
                title={player.track?.id === track.id ? 'Jump to cue' : 'Load the track to jump to cues'}
                onclick={() => onjump(cue.start / 1000)}
              >
                {#if cue.kind === 'hot'}
                  <span
                    class="grid size-5 place-items-center rounded text-[10px] font-bold text-bg"
                    style:background={cueColor(cue)}>{hotCueLetter(cue.slot)}</span
                  >
                {:else}
                  <span class="grid size-5 place-items-center">
                    <span class="size-2 rotate-45 rounded-[1px]" style:background={cueColor(cue)}></span>
                  </span>
                {/if}
                <span class="font-mono text-muted">{formatClock(cue.start)}</span>
                {#if cue.end != null}
                  <Repeat class="size-3 text-faint" />
                  <span class="font-mono text-faint">{formatClock(cue.end)}</span>
                {/if}
                <span class="min-w-0 flex-1 truncate text-right text-faint">{cue.comment}</span>
              </button>
              {#if canRemoveCues}
                <button
                  class="mr-1 grid size-5 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-danger disabled:hidden"
                  disabled={player.editing}
                  onclick={() => player.cues.deleteCue(cue)}
                  aria-label={removeCueLabel(cue)}
                  title="Remove cue"
                >
                  <X class="size-3" />
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {:else if detail}
        <p class="text-[12px] text-faint">No cues set</p>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet file()}
  {#if track}
    <div class="mt-auto border-t border-line-soft px-4 py-3">
      {#if track.collected === false}
        <p class="mb-2 text-[11px] text-faint">Not in the collection</p>
      {:else if track.missing}
        <p class="mb-2 flex items-center gap-1.5 text-[11px] text-danger">
          <AlertTriangle class="size-3.5" /> File missing on disk
        </p>
      {/if}
      <button
        class="group flex w-full items-start gap-2 rounded-md text-left text-[11px] text-faint hover:text-muted"
        onclick={() => api.showItemInFolder(track.path)}
        title="Show in {window.api.platform === 'darwin' ? 'Finder' : 'Explorer'}"
      >
        <FolderSearch class="mt-px size-3.5 shrink-0" />
        <span class="break-all">{track.path}</span>
      </button>
    </div>
  {/if}
{/snippet}
