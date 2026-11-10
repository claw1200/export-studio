<script lang="ts">
  import { getApp } from '$lib/context'
  import { getPlayer } from '$lib/deck/context'
  import { formatBpm, formatKeyChange, formatTempoChange, keyColor } from '$lib/format'
  import { heardKey, heardSemitones } from '$lib/mix/harmonic'
  import { stepTempo } from '$lib/mix/sync'
  import Stepper, { type Reading } from './Stepper.svelte'

  const { prefs } = getApp().decks
  const player = getPlayer()
  const tempo = player.tempo
  const track = $derived(player.track)
  const bpm = $derived(player.trackBpmAtPlayhead)
  const key = $derived(heardKey(track?.key ?? '', tempo))
  const atTrackTempo = $derived(tempo.rate === 1)
  const TEMPO_PERCENT_STEP = 0.1

  function stepBpm(step: number): void {
    const rate = stepTempo(tempo.rate, bpm, step)
    if (rate != null) tempo.setRate(rate)
  }

  function stepTempoPercent(steps: number): void {
    tempo.setPercent(Math.round(tempo.percent / TEMPO_PERCENT_STEP + steps) * TEMPO_PERCENT_STEP)
  }

  const stepKey = (steps: number): void => tempo.setKeyShift(tempo.keyShift + steps)

  const CHANGE_MEANING = 'the change from the track'

  function inReadoutOrder(absolute: Reading, relative: Reading): { reading: Reading; shiftReading: Reading } {
    return prefs.tempoReadout === 'relative'
      ? { reading: relative, shiftReading: absolute }
      : { reading: absolute, shiftReading: relative }
  }
</script>

<Stepper
  class="w-[120px]"
  name="BPM"
  {...inReadoutOrder(
    { text: track && bpm ? formatBpm(bpm * tempo.rate) : '—', meaning: 'the BPM', stepSize: '1', onstep: stepBpm },
    {
      text: formatTempoChange(tempo.percent),
      meaning: CHANGE_MEANING,
      stepSize: `${TEMPO_PERCENT_STEP}%`,
      onstep: stepTempoPercent
    }
  )}
  valueClass={atTrackTempo ? 'text-fg' : 'text-accent'}
  title={tempo.lockedToMaster
    ? 'Following the master deck'
    : atTrackTempo
      ? 'At the track’s own tempo'
      : `${formatBpm(bpm)} BPM at the track’s own tempo. Click to go back to it.`}
  disabled={!track || tempo.lockedToMaster || !(bpm > 0)}
  onreset={() => tempo.reset()}
/>
<Stepper
  class="w-[104px]"
  name="Key"
  {...inReadoutOrder(
    { text: key || '—', meaning: 'the key', stepSize: '1', onstep: stepKey },
    { text: formatKeyChange(heardSemitones(tempo)), meaning: CHANGE_MEANING, stepSize: '1', onstep: stepKey }
  )}
  color={key ? keyColor(key) : null}
  title={!track?.key
    ? 'No key detected'
    : key === track.key
      ? 'At the track’s own key'
      : `${track.key} at the track’s own pitch. Click to clear the key shift.`}
  disabled={!track}
  onreset={() => tempo.setKeyShift(0)}
/>
