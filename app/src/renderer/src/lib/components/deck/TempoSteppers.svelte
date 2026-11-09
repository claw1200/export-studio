<script lang="ts">
  import { getPlayer } from '$lib/deck/context'
  import { formatBpm, keyColor } from '$lib/format'
  import { heardKey } from '$lib/mix/harmonic'
  import { stepTempo } from '$lib/mix/sync'
  import Stepper from './Stepper.svelte'

  let {
    stepperClass,
    lockedBy = null
  }: {
    stepperClass?: string
    /** Why the tempo cannot be moved here, as when following the master. */
    lockedBy?: string | null
  } = $props()

  const player = getPlayer()
  const tempo = player.tempo
  const track = $derived(player.track)
  const bpm = $derived(player.bpmUnderPlayhead)
  const key = $derived(heardKey(track?.key ?? '', tempo))
  const signed = (n: number): string => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '')

  function stepBpm(step: number): void {
    const rate = stepTempo(tempo.rate, bpm, step)
    if (rate != null) tempo.setRate(rate)
  }
</script>

<Stepper
  class={stepperClass}
  name="BPM"
  value={track && bpm ? formatBpm(bpm * tempo.rate) : '—'}
  valueClass={tempo.rate !== 1 ? 'text-accent' : 'text-fg'}
  title={lockedBy ??
    (tempo.rate === 1
      ? 'At the track’s own tempo'
      : `${formatBpm(bpm)} BPM at the track’s own tempo. Click to go back to it.`)}
  disabled={!track || !!lockedBy || !(bpm > 0)}
  onstep={stepBpm}
  onreset={() => tempo.reset()}
/>
<Stepper
  class={stepperClass}
  name="Key"
  value={key || '—'}
  color={key ? keyColor(key) : null}
  title={!track?.key
    ? 'No key detected'
    : key === track.key
      ? 'At the track’s own key'
      : `${track.key} at the track’s own pitch. Click to clear the key shift.`}
  disabled={!track}
  onstep={(step) => tempo.setKeyShift(tempo.keyShift + step)}
  onreset={() => tempo.setKeyShift(0)}
  badge={signed(tempo.keyShift)}
/>
