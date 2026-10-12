<script lang="ts">
  import { getApp } from '$lib/context'
  import type { Track } from '$lib/types'
  import ConfirmDialog from './ConfirmDialog.svelte'

  let { tracks, onclose }: { tracks: Track[]; onclose: () => void } = $props()

  const { library } = getApp()
  const trashName = window.api.platform === 'win32' ? 'Recycle Bin' : 'Trash'

  let trash = $state(false)

  const message = $derived(
    tracks.length === 1
      ? `Remove “${tracks[0].title}” from the collection and every playlist? Its cues, beatgrid, and stored file type are deleted. Importing the file again reads the type from the audio file.`
      : `Remove ${tracks.length} tracks from the collection and every playlist? Their cues, beatgrids, and stored file types are deleted. Importing the files again reads the type from each audio file.`
  )

  function confirm(): void {
    void library.removeFromCollection(
      tracks.map((t) => t.id),
      trash
    )
  }
</script>

<ConfirmDialog
  title="Remove from collection"
  {message}
  confirmLabel={trash ? `Remove and move to ${trashName}` : 'Remove'}
  danger
  onconfirm={confirm}
  {onclose}
>
  <label class="mt-3 flex items-center gap-2 text-[12.5px] text-muted">
    <input type="checkbox" class="accent-danger" bind:checked={trash} />
    Also move the audio {tracks.length === 1 ? 'file' : 'files'} to the {trashName}
  </label>
</ConfirmDialog>
