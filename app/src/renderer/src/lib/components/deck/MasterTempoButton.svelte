<script lang="ts">
  import { getPlayer } from '$lib/deck/context'
  import { segmentButton, segmentGroup } from './segment'

  let { rememberAsDefault = false }: { rememberAsDefault?: boolean } = $props()

  const player = getPlayer()
  const tempo = player.tempo
  const on = $derived(tempo.masterTempo)

  function toggle(): void {
    if (rememberAsDefault) tempo.setMasterTempoAndDefault(!on)
    else tempo.setMasterTempo(!on)
  }
</script>

<div class={segmentGroup}>
  <button
    class={segmentButton(on, 'accent')}
    disabled={!player.track}
    aria-pressed={on}
    aria-label="Master tempo"
    title={on
      ? 'Master tempo on: the key holds when the tempo moves'
      : 'Master tempo off: the key moves with the tempo'}
    onclick={toggle}>MT</button
  >
</div>
