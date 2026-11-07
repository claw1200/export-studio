<script lang="ts">
  import { getPlayer } from '$lib/deck/context'
  import { formatBpm, keyColor } from '$lib/format'
  import { heardKey } from '$lib/mix/harmonic'
  import { stepTempo } from '$lib/mix/sync'
  import Stepper from './Stepper.svelte'

  /**
   * The tempo and key as they are playing now, stepped as on the mix deck: the BPM by 1 at a time,
   * the key by a semitone. Playback only: the beatgrid editor sets the grid's own BPM.
   */
  const { bpm }: { /** At the playhead, at the track's own tempo. */ bpm: number } = $props()

  const player = getPlayer()
  const tempo = player.tempo
  const track = $derived(player.track)
  const rate = $derived(tempo.rate)
  const keyShift = $derived(tempo.keyShift)
  const key = $derived(heardKey(track?.key ?? '', tempo))
  const signed = (n: number): string => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '')

  function stepBpm(step: number): void {
    const next = stepTempo(bpm * rate, bpm, step)
    if (next != null) tempo.setRate(next)
  }
</script>

<div class="flex shrink-0 flex-col gap-1">
  <Stepper
    class="w-[124px]"
    name="BPM"
    value={track && bpm ? formatBpm(bpm * rate) : '—'}
    valueClass={rate !== 1 ? 'text-accent' : 'text-fg'}
    title={rate === 1
      ? 'At the track’s own tempo'
      : `${formatBpm(bpm)} BPM at the track’s own tempo. Click to go back to it.`}
    disabled={!track || !(bpm > 0)}
    onstep={stepBpm}
    onreset={() => tempo.setRate(1)}
  />
  <Stepper
    class="w-[124px]"
    name="Key"
    value={key || '—'}
    color={key ? keyColor(key) : null}
    title={!track?.key
      ? 'No key detected'
      : key === track.key
        ? 'At the track’s own key'
        : `${track.key} at the track’s own pitch. Click to clear the key shift.`}
    disabled={!track}
    onstep={(step) => tempo.setKeyShift(keyShift + step)}
    onreset={() => tempo.setKeyShift(0)}
    badge={signed(keyShift)}
  />
</div>
