<script lang="ts">
  import { untrack } from 'svelte'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import { api } from '$lib/api'
  import { emptyDuplicateMatch } from '$lib/library/duplicates'
  import { emptyFilters, formatDurationInput, parseDurationInput } from '$lib/library/views'
  import type { DuplicateMatch, DurationSlack, PlaylistMembership, TrackViewFilters, View } from '$lib/types'

  let { view = null, onclose }: { view?: View | null; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  const opened = untrack(() => view)
  const initial = opened?.type === 'tracks' ? opened.filters : emptyFilters()
  const initialMatch = opened?.type === 'duplicates' ? opened.match : emptyDuplicateMatch()
  let name = $state(opened?.name ?? '')
  let kind = $state<'tracks' | 'files' | 'duplicates'>(
    opened?.type === 'tracks' || opened?.type === 'files' || opened?.type === 'duplicates' ? opened.type : 'tracks'
  )
  let filters = $state<TrackViewFilters>({ ...initial, keys: [...initial.keys] })
  let match = $state<DuplicateMatch>({ ...initialMatch })
  let durationSlack = $state<DurationSlack>(opened?.type === 'duplicates' ? opened.durationSlack : 2)
  let directory = $state(opened?.type === 'files' ? opened.directory : '')
  let showInCollection = $state(opened?.type === 'files' ? opened.showInCollection : true)
  let bpmMin = $state(initial.bpmMin == null ? '' : String(initial.bpmMin))
  let bpmMax = $state(initial.bpmMax == null ? '' : String(initial.bpmMax))
  let timeMin = $state(formatDurationInput(initial.durationMin))
  let timeMax = $state(formatDurationInput(initial.durationMax))
  let error = $state('')

  const creating = $derived(!view)
  const keys = $derived.by(() => {
    const found: string[] = []
    for (const track of library.tracks) {
      const key = track.key.trim()
      if (key && !found.includes(key)) found.push(key)
    }
    return found.sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
  })

  const memberships: { id: PlaylistMembership; label: string }[] = [
    { id: 'any', label: 'Any' },
    { id: 'in', label: 'In a playlist' },
    { id: 'out', label: 'Not in a playlist' }
  ]

  const slacks: { value: DurationSlack; label: string }[] = [
    { value: 0, label: 'Exact' },
    { value: 1, label: '1s' },
    { value: 2, label: '2s' },
    { value: 5, label: '5s' }
  ]

  const blurbs = {
    tracks: 'List collection tracks that match every filter you fill in.',
    files: 'List the audio files under a folder. Files already in the collection keep their library data.',
    duplicates: 'List collection tracks that match each other on every field you turn on.'
  }
  const blurb = $derived(blurbs[kind])

  const inputClass =
    'h-7 w-full rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg placeholder:text-faint focus:border-accent/60'

  async function browse(): Promise<void> {
    const picked = await api.choosePath('folder', directory || undefined)
    if (picked) {
      directory = picked
      error = ''
    }
  }

  function toggleKey(key: string): void {
    filters.keys = filters.keys.includes(key) ? filters.keys.filter((item) => item !== key) : [...filters.keys, key]
  }

  function parseBpm(text: string): number | null | undefined {
    const trimmed = text.trim()
    if (!trimmed) return null
    const value = Number(trimmed)
    if (!Number.isFinite(value) || value < 0) return undefined
    return value
  }

  function ordered(min: number | null, max: number | null): { min: number | null; max: number | null } {
    if (min != null && max != null && min > max) return { min: max, max: min }
    return { min, max }
  }

  function save(): boolean {
    const trimmed = name.trim()
    if (!trimmed) {
      error = 'A view needs a name'
      return false
    }
    const id = opened?.id ?? crypto.randomUUID()
    if (kind === 'files') {
      const folder = directory.trim()
      if (!folder) {
        error = 'Choose a folder to list'
        return false
      }
      library.saveView({ id, name: trimmed, type: 'files', directory: folder, showInCollection })
      return true
    }
    if (kind === 'duplicates') {
      if (!match.title && !match.artist && !match.duration && !match.fileSize) {
        error = 'Turn on at least one field'
        return false
      }
      library.saveView({
        id,
        name: trimmed,
        type: 'duplicates',
        match: {
          title: match.title,
          artist: match.artist,
          duration: match.duration,
          fileSize: match.fileSize
        },
        durationSlack
      })
      return true
    }
    const low = parseBpm(bpmMin)
    const high = parseBpm(bpmMax)
    if (low === undefined || high === undefined) {
      error = 'BPM must be a number'
      return false
    }
    const durationMin = parseDurationInput(timeMin)
    const durationMax = parseDurationInput(timeMax)
    if (durationMin === undefined || durationMax === undefined) {
      error = 'Time is minutes and seconds, such as 6:30'
      return false
    }
    const bpm = ordered(low, high)
    const duration = ordered(durationMin, durationMax)
    library.saveView({
      id,
      name: trimmed,
      type: 'tracks',
      filters: {
        title: filters.title.trim(),
        artist: filters.artist.trim(),
        album: filters.album.trim(),
        genre: filters.genre.trim(),
        label: filters.label.trim(),
        comment: filters.comment.trim(),
        bpmMin: bpm.min,
        bpmMax: bpm.max,
        durationMin: duration.min,
        durationMax: duration.max,
        keys: [...filters.keys],
        playlist: filters.playlist
      }
    })
    return true
  }
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto max-h-[calc(100vh-48px)] w-[520px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label={creating ? 'New view' : 'Edit view'}
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <form
    method="dialog"
    class="flex max-h-[calc(100vh-48px)] flex-col"
    onsubmit={(e) => {
      if (e.submitter?.getAttribute('value') !== 'confirm') return
      if (!save()) e.preventDefault()
    }}
  >
    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      <h2 class="text-[13.5px] font-semibold">{creating ? 'New view' : 'Edit view'}</h2>
      <p class="mt-1 text-[12.5px] leading-relaxed text-muted">{blurb}</p>

      <label class="mt-4 block">
        <span class="mb-1 block text-[11px] font-medium text-faint">Name</span>
        <input
          bind:value={name}
          class={inputClass}
          spellcheck="false"
          placeholder={kind === 'duplicates' ? 'Suspected duplicates' : ''}
        />
      </label>

      <div class="mt-4 flex rounded-md bg-bg p-0.5" role="radiogroup" aria-label="View type">
        <button
          type="button"
          class={[
            'h-7 flex-1 rounded-[5px] text-[12.5px]',
            kind === 'tracks' ? 'bg-raised font-medium text-fg' : 'text-muted hover:text-fg'
          ]}
          role="radio"
          aria-checked={kind === 'tracks'}
          onclick={() => (kind = 'tracks')}
        >
          Track list
        </button>
        <button
          type="button"
          class={[
            'h-7 flex-1 rounded-[5px] text-[12.5px]',
            kind === 'files' ? 'bg-raised font-medium text-fg' : 'text-muted hover:text-fg'
          ]}
          role="radio"
          aria-checked={kind === 'files'}
          onclick={() => (kind = 'files')}
        >
          File list
        </button>
        <button
          type="button"
          class={[
            'h-7 flex-1 rounded-[5px] text-[12.5px]',
            kind === 'duplicates' ? 'bg-raised font-medium text-fg' : 'text-muted hover:text-fg'
          ]}
          role="radio"
          aria-checked={kind === 'duplicates'}
          onclick={() => (kind = 'duplicates')}
        >
          Duplicates
        </button>
      </div>

      {#if kind === 'tracks'}
        <div class="mt-4 grid grid-cols-2 gap-3">
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Title</span>
            <input bind:value={filters.title} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Artist</span>
            <input bind:value={filters.artist} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Album</span>
            <input bind:value={filters.album} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Genre</span>
            <input bind:value={filters.genre} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Label</span>
            <input bind:value={filters.label} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Comment</span>
            <input bind:value={filters.comment} class={inputClass} spellcheck="false" placeholder="Contains" />
          </label>
        </div>

        <div class="mt-3 grid grid-cols-2 gap-3">
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">BPM from</span>
            <input bind:value={bpmMin} class={inputClass} inputmode="decimal" placeholder="Any" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">BPM to</span>
            <input bind:value={bpmMax} class={inputClass} inputmode="decimal" placeholder="Any" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Time from</span>
            <input bind:value={timeMin} class={inputClass} spellcheck="false" placeholder="0:00" />
          </label>
          <label class="block">
            <span class="mb-1 block text-[11px] font-medium text-faint">Time to</span>
            <input bind:value={timeMax} class={inputClass} spellcheck="false" placeholder="0:00" />
          </label>
        </div>

        <div class="mt-4">
          <span class="mb-1.5 block text-[11px] font-medium text-faint">Key</span>
          {#if keys.length}
            <div class="flex flex-wrap gap-1">
              {#each keys as key (key)}
                {@const on = filters.keys.includes(key)}
                <button
                  type="button"
                  class={[
                    'h-6 rounded-md px-2 text-[12px]',
                    on ? 'bg-accent text-bg' : 'bg-bg text-muted hover:bg-hover hover:text-fg'
                  ]}
                  aria-pressed={on}
                  onclick={() => toggleKey(key)}
                >
                  {key}
                </button>
              {/each}
            </div>
          {:else}
            <p class="text-[12px] text-faint">No keys in the collection yet</p>
          {/if}
        </div>

        <div class="mt-4">
          <span class="mb-1.5 block text-[11px] font-medium text-faint">Playlist</span>
          <div class="flex rounded-md bg-bg p-0.5" role="radiogroup" aria-label="Playlist">
            {#each memberships as item (item.id)}
              <button
                type="button"
                class={[
                  'h-7 flex-1 rounded-[5px] text-[12px]',
                  filters.playlist === item.id ? 'bg-raised font-medium text-fg' : 'text-muted hover:text-fg'
                ]}
                role="radio"
                aria-checked={filters.playlist === item.id}
                onclick={() => (filters.playlist = item.id)}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </div>
      {:else if kind === 'files'}
        <div class="mt-4">
          <span class="mb-1 block text-[11px] font-medium text-faint">Folder</span>
          <div class="flex gap-2">
            <input
              value={directory}
              readonly
              class="{inputClass} min-w-0 flex-1"
              placeholder="No folder chosen"
              aria-label="Folder"
            />
            <button
              type="button"
              class="h-7 shrink-0 rounded-md bg-raised px-3 text-[12.5px] text-fg hover:bg-hover"
              onclick={() => void browse()}
            >
              Browse…
            </button>
          </div>
          <p class="mt-1.5 text-[11.5px] leading-snug text-faint">
            Includes audio files in subfolders. Hidden folders are skipped.
          </p>
        </div>
        <label class="mt-4 flex items-center gap-2 text-[12.5px] text-muted">
          <input type="checkbox" class="accent-accent" bind:checked={showInCollection} />
          Show files already in the collection
        </label>
      {:else if kind === 'duplicates'}
        <div class="mt-4 grid grid-cols-2 gap-2">
          <label class="flex items-center gap-2 text-[12.5px] text-muted">
            <input type="checkbox" class="accent-accent" bind:checked={match.title} />
            Same title
          </label>
          <label class="flex items-center gap-2 text-[12.5px] text-muted">
            <input type="checkbox" class="accent-accent" bind:checked={match.artist} />
            Same artist
          </label>
          <label class="flex items-center gap-2 text-[12.5px] text-muted">
            <input type="checkbox" class="accent-accent" bind:checked={match.duration} />
            Duration
          </label>
          <label class="flex items-center gap-2 text-[12.5px] text-muted">
            <input type="checkbox" class="accent-accent" bind:checked={match.fileSize} />
            Same file size
          </label>
        </div>
        <p class="mt-3 text-[11.5px] leading-snug text-faint">
          Filter tracks that are suspected to be duplicates of each other.
        </p>
        {#if match.title || match.fileSize}
          <p class="mt-1.5 text-[11.5px] leading-snug text-faint">
            {[
              match.title ? 'Tracks with no title are left out.' : '',
              match.fileSize ? 'Tracks with an unknown size are left out.' : ''
            ]
              .filter(Boolean)
              .join(' ')}
          </p>
        {/if}
        {#if match.duration && !match.title && !match.artist && !match.fileSize}
          <p class="mt-1.5 text-[11.5px] leading-snug text-faint">
            Duration alone groups every track of a similar length. Turn on title, artist, or file size to narrow it.
          </p>
        {/if}
        {#if match.duration}
          <div class="mt-4">
            <span class="mb-1.5 block text-[11px] font-medium text-faint">Duration tolerance</span>
            <div class="flex rounded-md bg-bg p-0.5" role="radiogroup" aria-label="Duration tolerance">
              {#each slacks as slack (slack.value)}
                <button
                  type="button"
                  class={[
                    'h-7 flex-1 rounded-[5px] text-[12px]',
                    durationSlack === slack.value ? 'bg-raised font-medium text-fg' : 'text-muted hover:text-fg'
                  ]}
                  role="radio"
                  aria-checked={durationSlack === slack.value}
                  onclick={() => (durationSlack = slack.value)}
                >
                  {slack.label}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/if}

      {#if error}
        <p class="mt-3 text-[12px] text-danger" role="alert">{error}</p>
      {/if}
    </div>

    <div class="flex justify-end gap-2 border-t border-line-soft px-5 py-3">
      <button value="cancel" class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg">
        Cancel
      </button>
      <button
        value="confirm"
        class="h-7 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90"
      >
        {creating ? 'Create' : 'Save'}
      </button>
    </div>
  </form>
</dialog>
