<script lang="ts">
  import { presetLabel } from '$lib/convertPresets'
  import { getApp } from '$lib/context'
  import type { ConvertMode, ConvertPreset, Track } from '$lib/types'
  import ConfirmDialog from './ConfirmDialog.svelte'

  let { tracks, preset, onclose }: { tracks: Track[]; preset: ConvertPreset; onclose: () => void } = $props()

  const { library } = getApp()
  const trashName = window.api.platform === 'win32' ? 'Recycle Bin' : 'Trash'

  let mode = $state<ConvertMode>('update')

  const target = $derived(presetLabel(preset))
  const subject = $derived(tracks.length === 1 ? `“${tracks[0].title}”` : `${tracks.length} tracks`)
  const MODES = $derived<{ id: ConvertMode; label: string; help: string }[]>([
    { id: 'copy', label: 'Create a copy', help: 'Adds a new track with the same cues and beatgrid, using the new file.' },
    { id: 'update', label: 'Update track', help: 'Points the track at the new file. The original file stays on disk.' },
    {
      id: 'update_delete',
      label: 'Update and remove original',
      help: `Points the track at the new file and moves the original to the ${trashName}.`
    }
  ])

  function confirm(): void {
    void library.convertTracks(
      tracks.map((t) => t.id),
      preset,
      mode
    )
  }
</script>

<ConfirmDialog
  title="Convert file type"
  message={`Convert ${subject} to ${target}?`}
  confirmLabel="Convert"
  danger={mode === 'update_delete'}
  onconfirm={confirm}
  {onclose}
>
  <div class="mt-3 space-y-1.5" role="radiogroup" aria-label="After converting">
    {#each MODES as m (m.id)}
      <label class="flex items-start gap-2 rounded-md px-1 py-1 text-[12.5px] hover:bg-hover">
        <input type="radio" class="mt-0.5 accent-accent" name="convert-mode" value={m.id} bind:group={mode} />
        <span class="min-w-0">
          <span class="block font-medium">{m.label}</span>
          <span class="block text-[11px] leading-snug text-faint">{m.help}</span>
        </span>
      </label>
    {/each}
  </div>
</ConfirmDialog>
