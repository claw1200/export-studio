<script lang="ts">
  import { Music, FolderSearch, Repeat, AlertTriangle, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { api, artworkUrl } from '$lib/api'
  import { formatBpm, formatClock, formatDuration, formatSize, hotCueLetter, keyColor, TRACK_COLORS } from '$lib/format'
  import { loadDetail } from '$lib/player.svelte'
  import { cueColor } from '$lib/waveform'
  import type { Cue, TrackDetail } from '$lib/types'

  let { onjump }: { onjump: (seconds: number) => void } = $props()

  const { library, player } = getApp()
  const track = $derived(library.selected)

  const detailPromise = $derived(track ? loadDetail(track.id).then((d) => d.detail) : null)

  const fields = $derived.by(() => {
    if (!track) return []
    const rows: [string, string][] = [
      ['Album', track.album],
      ['Genre', track.genre],
      ['Label', track.label],
      ['Remixer', track.remixer],
      ['Original artist', track.originalArtist],
      ['Composer', track.composer],
      ['Year', track.year ? String(track.year) : ''],
      ['Date added', track.dateAdded],
      ['Play count', track.playCount ? String(track.playCount) : ''],
      ['Colour', TRACK_COLORS[track.color]?.name ?? ''],
      [
        'Format',
        [
          track.fileType,
          track.sampleRate ? `${(track.sampleRate / 1000).toFixed(1)} kHz` : '',
          track.bitDepth ? `${track.bitDepth}-bit` : '',
          track.bitrate ? `${track.bitrate} kbps` : ''
        ]
          .filter(Boolean)
          .join(' · ')
      ],
      ['Size', formatSize(track.fileSize)]
    ]
    return rows.filter(([, v]) => v)
  })

  const tags = $derived(track ? track.myTags.map((id) => library.tagNames.get(id)).filter(Boolean) : [])
  const canRemoveCues = $derived(library.writable && !!player.detail && player.track?.id === track?.id)

  function removeCueLabel(cue: Cue): string {
    if (cue.kind === 'hot') return `Remove hot cue ${hotCueLetter(cue.slot)}`
    return `Remove memory cue at ${formatClock(cue.start)}`
  }
</script>

<aside class="flex h-full w-full flex-col overflow-y-auto border-l border-line-soft bg-bg">
  {#if track}
    <div class="p-4">
      <div class="aspect-square w-full overflow-hidden rounded-lg bg-raised shadow-xl shadow-black/40">
        {#if track.hasArtwork}
          <img src={artworkUrl(track.id, '')} alt="" class="size-full object-cover" draggable="false" />
        {:else}
          <div class="grid size-full place-items-center text-faint"><Music class="size-10" /></div>
        {/if}
      </div>
      <h2 class="mt-4 text-[16px] leading-snug font-semibold text-fg select-text">{track.title}</h2>
      <p class="mt-0.5 text-[13px] text-muted select-text">{track.artist || 'Unknown artist'}</p>

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

    <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 border-t border-line-soft px-4 py-3 text-[12px]">
      {#each fields as [label, value] (label)}
        <dt class="text-faint">{label}</dt>
        <dd class="min-w-0 truncate text-right text-muted select-text" title={value}>{value}</dd>
      {/each}
    </dl>

    {#if tags.length}
      <div class="flex flex-wrap gap-1 border-t border-line-soft px-4 py-3">
        {#each tags as tag (tag)}
          <span class="rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-medium text-accent">{tag}</span>
        {/each}
      </div>
    {/if}

    {#if track.comment}
      <p class="border-t border-line-soft px-4 py-3 text-[12px] text-muted select-text">{track.comment}</p>
    {/if}

    {#await detailPromise}
      {@render cues(null)}
      {@render file(null)}
    {:then detail}
      {@render cues(player.detail?.id === detail.id ? player.detail : detail)}
      {@render file(detail)}
    {:catch}
      {@render file(null)}
    {/await}
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
                  onclick={() => player.deleteCue(cue)}
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

{#snippet file(detail: TrackDetail | null)}
  {#if track}
    <div class="mt-auto border-t border-line-soft px-4 py-3">
      {#if detail && !detail.fileExists}
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
