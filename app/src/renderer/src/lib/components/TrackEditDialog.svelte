<script lang="ts">
  import { untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { Music, Star, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import { TRACK_COLORS } from '$lib/format'
  import type { EditableField, Track, TrackEdit } from '$lib/types'

  let { tracks, onclose }: { tracks: Track[]; onclose: () => void } = $props()

  const { library, keybinds } = getApp()

  type TextField = Exclude<EditableField, 'year' | 'trackNo' | 'rating' | 'color'>
  type Suggestions = 'artists' | 'albums' | 'genres' | 'labels'
  type TextSpec = { id: TextField; label: string; list?: Suggestions; wide?: boolean; single?: boolean }

  const TEXT_FIELDS: TextSpec[] = [
    { id: 'title', label: 'Title', wide: true, single: true },
    { id: 'artist', label: 'Artist', list: 'artists', wide: true },
    { id: 'album', label: 'Album', list: 'albums', wide: true },
    { id: 'genre', label: 'Genre', list: 'genres' },
    { id: 'label', label: 'Label', list: 'labels' },
    { id: 'remixer', label: 'Remixer', list: 'artists' },
    { id: 'originalArtist', label: 'Original artist', list: 'artists' },
    { id: 'composer', label: 'Composer', list: 'artists' }
  ]
  const NUMBER_FIELDS: { id: 'year' | 'trackNo'; label: string; single?: boolean }[] = [
    { id: 'year', label: 'Year' },
    { id: 'trackNo', label: 'Track no.', single: true }
  ]
  const MAX_NUMBER = 9999

  const ids = $derived(tracks.map((t) => t.id))
  const single = $derived(tracks.length === 1 ? tracks[0] : null)

  function shared<K extends EditableField>(list: Track[], key: K): Track[K] | undefined {
    const first = list[0]?.[key]
    return list.every((t) => t[key] === first) ? first : undefined
  }

  const start = untrack(() => {
    const text = (key: EditableField): string => {
      const v = shared(tracks, key)
      return v === undefined || v === 0 ? '' : String(v)
    }
    return {
      values: {
        title: text('title'),
        artist: text('artist'),
        album: text('album'),
        genre: text('genre'),
        label: text('label'),
        remixer: text('remixer'),
        originalArtist: text('originalArtist'),
        composer: text('composer'),
        comment: text('comment'),
        year: text('year'),
        trackNo: text('trackNo')
      } satisfies Record<Exclude<EditableField, 'rating' | 'color'>, string>,
      rating: shared(tracks, 'rating') ?? null,
      color: shared(tracks, 'color') ?? null,
      mixed: new Set(
        (['comment', 'year', 'trackNo', 'rating', 'color', ...TEXT_FIELDS.map((f) => f.id)] as EditableField[]).filter(
          (key) => shared(tracks, key) === undefined
        )
      )
    }
  })

  const values = $state(start.values)
  let rating = $state<number | null>(start.rating)
  let color = $state<number | null>(start.color)
  const dirty = new SvelteSet<EditableField>()
  let saving = $state(false)
  let error = $state<string | null>(null)

  const suggestions = $derived.by(() => {
    const sets: Record<Suggestions, Set<string>> = {
      artists: new Set(),
      albums: new Set(),
      genres: new Set(),
      labels: new Set()
    }
    for (const t of library.tracks) {
      for (const name of [t.artist, t.remixer, t.originalArtist, t.composer]) if (name) sets.artists.add(name)
      if (t.album) sets.albums.add(t.album)
      if (t.genre) sets.genres.add(t.genre)
      if (t.label) sets.labels.add(t.label)
    }
    const sorted = (s: Set<string>): string[] => [...s].sort((a, b) => a.localeCompare(b))
    return {
      artists: sorted(sets.artists),
      albums: sorted(sets.albums),
      genres: sorted(sets.genres),
      labels: sorted(sets.labels)
    }
  })

  function placeholder(key: EditableField): string {
    return start.mixed.has(key) && !dirty.has(key) ? 'Mixed' : ''
  }

  function setText(key: Exclude<EditableField, 'rating' | 'color'>, value: string): void {
    values[key] = value
    dirty.add(key)
    error = null
  }

  function setRating(n: number): void {
    rating = rating === n ? 0 : n
    dirty.add('rating')
  }

  function setColor(n: number): void {
    color = n
    dirty.add('color')
  }

  function parseNumber(key: 'year' | 'trackNo', label: string): number | null {
    const raw = values[key].trim()
    if (!raw) return 0
    const n = Number(raw)
    if (!Number.isInteger(n) || n < 0 || n > MAX_NUMBER) {
      error = `${label} must be a whole number between 0 and ${MAX_NUMBER}`
      return null
    }
    return n
  }

  async function save(dialog: HTMLDialogElement): Promise<void> {
    const fields: TrackEdit = {}
    for (const key of dirty) {
      if (key === 'rating') fields.rating = rating ?? 0
      else if (key === 'color') fields.color = color ?? 0
      else if (key === 'year' || key === 'trackNo') {
        const n = parseNumber(key, NUMBER_FIELDS.find((f) => f.id === key)?.label ?? key)
        if (n === null) return
        fields[key] = n
      } else fields[key] = values[key].trim()
    }
    if (single && 'title' in fields && !fields.title) {
      error = 'A track needs a title'
      return
    }
    saving = true
    error = null
    const ok = await library.updateTracks(ids, fields)
    saving = false
    if (ok) dialog.close()
    else error = library.editError ?? 'Couldn’t save the changes'
  }

  function modal(dialog: HTMLDialogElement): () => void {
    const resume = keybinds.suspend()
    dialog.showModal()
    return resume
  }

  const inputClass =
    'h-7 w-full rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg placeholder:text-faint select-text focus:border-accent/60 focus:outline-none disabled:opacity-50'
</script>

<dialog
  {@attach modal}
  class="no-drag m-auto max-h-[calc(100vh-48px)] w-[560px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Edit track info"
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget && !saving) e.currentTarget.close()
  }}
>
  <form
    class="flex max-h-[calc(100vh-48px)] flex-col"
    onsubmit={(e) => {
      e.preventDefault()
      const dialog = e.currentTarget.closest('dialog')
      if (dialog) void save(dialog)
    }}
  >
    <header class="flex shrink-0 items-center gap-3 border-b border-line-soft px-5 py-3.5">
      <div class="size-10 shrink-0 overflow-hidden rounded-md bg-raised">
        {#if single?.hasArtwork}
          <img src={artworkUrl(single.id, 's')} alt="" class="size-full object-cover" draggable="false" />
        {:else}
          <div class="grid size-full place-items-center text-faint"><Music class="size-4" /></div>
        {/if}
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-[13.5px] font-semibold">
          {single ? single.title : `Edit ${tracks.length} tracks`}
        </h2>
        <p class="truncate text-[11.5px] text-faint" title={single?.path}>
          {single ? single.path : 'Only the fields you change are saved to every selected track.'}
        </p>
      </div>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
        aria-label="Close"
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="grid min-h-0 grid-cols-2 gap-x-3 gap-y-2.5 overflow-y-auto px-5 py-4">
      {#each TEXT_FIELDS as field (field.id)}
        {#if !field.single || single}
          <label class={['flex flex-col gap-1', field.wide && 'col-span-2']}>
            <span class="text-[11px] font-medium text-faint">{field.label}</span>
            <input
              class={inputClass}
              value={values[field.id]}
              placeholder={placeholder(field.id)}
              list={field.list ? `edit-${field.list}` : undefined}
              spellcheck="false"
              autocomplete="off"
              oninput={(e) => setText(field.id, e.currentTarget.value)}
            />
          </label>
        {/if}
      {/each}

      {#each NUMBER_FIELDS as field (field.id)}
        {#if !field.single || single}
          <label class="flex flex-col gap-1">
            <span class="text-[11px] font-medium text-faint">{field.label}</span>
            <input
              class={inputClass}
              value={values[field.id]}
              placeholder={placeholder(field.id)}
              inputmode="numeric"
              autocomplete="off"
              oninput={(e) => setText(field.id, e.currentTarget.value)}
            />
          </label>
        {/if}
      {/each}

      <div class="col-span-2 grid grid-cols-2 gap-x-3">
        <div class="flex flex-col gap-1">
          <span class="text-[11px] font-medium text-faint">
            Rating
            {#if start.mixed.has('rating') && !dirty.has('rating')}<span class="ml-1 font-normal">· Mixed</span>{/if}
          </span>
          <div class="flex h-7 items-center gap-0.5" role="radiogroup" aria-label="Rating">
            {#each [1, 2, 3, 4, 5] as n (n)}
              {@const lit = rating !== null && n <= rating}
              <button
                type="button"
                class="grid size-6 place-items-center rounded hover:bg-hover"
                role="radio"
                aria-checked={rating === n}
                aria-label="{n} star{n === 1 ? '' : 's'}"
                onclick={() => setRating(n)}
              >
                <Star class={['size-4', lit ? 'fill-warn text-warn' : 'text-line']} strokeWidth={lit ? 0 : 2} />
              </button>
            {/each}
          </div>
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-[11px] font-medium text-faint">
            Colour
            {#if start.mixed.has('color') && !dirty.has('color')}<span class="ml-1 font-normal">· Mixed</span>{/if}
          </span>
          <div class="flex h-7 items-center gap-1" role="radiogroup" aria-label="Colour">
            <button
              type="button"
              class={[
                'grid size-5 place-items-center rounded-full border text-faint',
                color === 0 ? 'border-fg' : 'border-line hover:border-muted'
              ]}
              role="radio"
              aria-checked={color === 0}
              aria-label="No colour"
              title="No colour"
              onclick={() => setColor(0)}
            >
              <X class="size-3" />
            </button>
            {#each Object.entries(TRACK_COLORS) as [n, c] (n)}
              <button
                type="button"
                class={[
                  'size-5 rounded-full ring-offset-2 ring-offset-surface',
                  color === Number(n) ? 'ring-2 ring-fg' : 'hover:ring-1 hover:ring-muted'
                ]}
                style:background={c.hex}
                role="radio"
                aria-checked={color === Number(n)}
                aria-label={c.name}
                title={c.name}
                onclick={() => setColor(Number(n))}
              ></button>
            {/each}
          </div>
        </div>
      </div>

      <label class="col-span-2 flex flex-col gap-1">
        <span class="text-[11px] font-medium text-faint">Comment</span>
        <textarea
          class={[inputClass, 'h-auto resize-none py-1.5 leading-snug']}
          rows="3"
          value={values.comment}
          placeholder={placeholder('comment')}
          spellcheck="false"
          oninput={(e) => setText('comment', e.currentTarget.value)}
        ></textarea>
      </label>
    </div>

    <footer class="flex shrink-0 items-center gap-3 border-t border-line-soft px-5 py-3 text-[11.5px]">
      {#if error}
        <span class="min-w-0 flex-1 truncate text-danger" title={error}>{error}</span>
      {:else if !library.writable}
        <span class="min-w-0 flex-1 truncate text-warn">Editing is locked. Unlock it to save changes.</span>
      {:else if library.writeFileTags}
        <span class="min-w-0 flex-1 truncate text-faint">Text fields are also written to the files’ tags.</span>
      {:else}
        <span class="flex-1"></span>
      {/if}
      <button
        type="button"
        class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      >
        Cancel
      </button>
      <button
        type="submit"
        class="h-7 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90 disabled:opacity-40"
        disabled={saving || !dirty.size || !library.writable}
      >
        {saving ? 'Saving…' : 'Save'}
      </button>
    </footer>
  </form>

  {#each Object.entries(suggestions) as [id, options] (id)}
    <datalist id="edit-{id}">
      {#each options as option (option)}
        <option value={option}></option>
      {/each}
    </datalist>
  {/each}
</dialog>
