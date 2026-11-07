<script lang="ts">
  import { SquarePen } from '@lucide/svelte'
  import { setDeckInfo, setPlayer } from '$lib/deck/context'
  import { formatBpm, keyColor, pitchSemitones } from '$lib/format'
  import { heardKey } from '$lib/mix/harmonic'
  import { ownBpm, playingBpm, type Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import WaveformOverview from '../WaveformOverview.svelte'
  import DeckShell from '../deck/DeckShell.svelte'
  import LoopControls from '../deck/LoopControls.svelte'
  import HotCueBar from '../deck/HotCueBar.svelte'
  import Stepper from '../deck/Stepper.svelte'
  import TrackInfo from '../deck/TrackInfo.svelte'
  import Transport from '../deck/Transport.svelte'

  /**
   * One side of the mixer, built from the same pieces as the edit deck: the track, the scrolling
   * waveform and an overview split into the sections between cues, with its hot cues under it to
   * play. Set the playhead by clicking the
   * overview or dragging the scrolling waveform; the mixer's play starts both, or the deck's own
   * play and cue start it alone. Under the track, play, cue and loops share a row with what only a
   * mix deck has: the master switch, tempo and key stepped by the mixer so a follower stays synced,
   * and master tempo. A narrow deck wraps the tempo side onto a second line.
   */
  let {
    player,
    mixer,
    label,
    focused,
    onfocus,
    onedit
  }: {
    /** Fixed for the component's life: key the deck on it to swap. */
    player: Player
    mixer: Mixer
    label: string
    /** Double-clicked rows load here. */
    focused: boolean
    onfocus: () => void
    /** Opens the deck's track in edit mode. */
    onedit: (track: Track) => void
  } = $props()

  // svelte-ignore state_referenced_locally
  setPlayer(player)
  /** Deck B's shortcuts are deck A's with Shift added: see `MixMode`. */
  // svelte-ignore state_referenced_locally
  setDeckInfo({ label, shifted: player === mixer.b })

  const track = $derived(player.track)
  const rate = $derived(player.tempo.rate)
  const bpm = $derived(playingBpm(player))
  const isMaster = $derived(mixer.master === player)
  const following = $derived(mixer.following(player))
  const masterTempo = $derived(player.tempo.masterTempo)
  const keyShift = $derived(player.tempo.keyShift)
  /** The key heard: the shift, and the tempo's pitch change too unless master tempo holds it. */
  const semitones = $derived(keyShift + (masterTempo ? 0 : pitchSemitones(rate)))
  const key = $derived(heardKey(track?.key ?? '', player.tempo))
  const bpmTitle = $derived(
    following
      ? `Following deck ${mixer.master === mixer.a ? 'A' : 'B'}`
      : rate === 1
        ? 'At the track’s own tempo'
        : `${formatBpm(ownBpm(player))} BPM at the track’s own tempo. Click to go back to it.`
  )
  const keyTitle = $derived(
    !track?.key
      ? 'No key detected'
      : semitones === 0
        ? 'At the track’s own key'
        : `${track.key} at the track’s own pitch. Click to clear the key shift.`
  )
  const signed = (n: number): string => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '')
</script>

<DeckShell
  class={[
    'flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-surface transition-colors',
    focused ? 'border-accent/60' : 'border-line-soft'
  ]}
  waveClass="h-[104px]"
  empty="Double-click or drag a track here to load it on deck {label}"
  onload={(loaded) => {
    onfocus()
    void player.load(loaded)
  }}
  {onfocus}
>
  {#snippet overlay()}
    <span
      class={[
        'pointer-events-none absolute top-2 left-2 grid size-6 place-items-center rounded text-[12px] font-bold',
        focused ? 'bg-accent text-bg' : 'bg-raised text-muted'
      ]}>{label}</span
    >
    {#if track}
      <button
        class="absolute top-2 right-2 z-10 grid size-6 place-items-center rounded bg-raised/80 text-muted transition-colors hover:bg-hover hover:text-fg"
        aria-label="Open deck {label}’s track in edit mode"
        title="Open in edit mode"
        onclick={() => onedit(track)}><SquarePen class="size-3.5" /></button
      >
    {/if}
  {/snippet}

  <TrackInfo class="px-3 pt-2.5" />

  <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-3 pt-2">
    <Transport
      onplay={() => mixer.toggleDeck(player)}
      playTitle="Play deck {label} on its own{following ? ', in time with the master' : ''}"
    />
    <div class="flex items-center gap-1.5">
      <LoopControls beatLoop={false} />
    </div>

    <div class="ml-auto flex items-center gap-2">
      <button
        class={[
          'h-6 rounded px-2 text-[10.5px] font-bold tracking-wide transition-colors disabled:opacity-40',
          isMaster ? 'bg-accent text-bg' : 'border border-line text-faint hover:bg-hover hover:text-muted'
        ]}
        disabled={!track}
        aria-pressed={isMaster}
        title={isMaster ? `Deck ${label} is the master` : `Make deck ${label} the master the other deck syncs to`}
        onclick={() => mixer.setMaster(player)}>MASTER</button
      >
      <Stepper
        name="BPM"
        value={track ? formatBpm(bpm) : '—'}
        valueClass={rate !== 1 ? 'text-accent' : 'text-fg'}
        title={bpmTitle}
        disabled={!track || following || !(ownBpm(player) > 0)}
        onstep={(step) => mixer.stepBpm(player, step)}
        onreset={() => mixer.resetTempo(player)}
      />
      <Stepper
        name="Key"
        value={key || '—'}
        color={key ? keyColor(key) : null}
        title={keyTitle}
        disabled={!track}
        onstep={(step) => mixer.stepKey(player, step)}
        onreset={() => mixer.resetKey(player)}
        badge={signed(keyShift)}
      />
      <button
        class={[
          'h-6 rounded px-1.5 text-[10.5px] font-bold transition-colors disabled:opacity-40',
          masterTempo ? 'bg-accent-soft text-accent' : 'border border-line text-faint hover:bg-hover hover:text-muted'
        ]}
        disabled={!track}
        aria-pressed={masterTempo}
        title={masterTempo
          ? 'Master tempo on: the key holds when the tempo moves'
          : 'Master tempo off: the key moves with the tempo'}
        onclick={() => mixer.toggleMasterTempo(player)}>MT</button
      >
    </div>
  </div>

  <div class="px-3 pt-2 pb-2.5">
    <div class="relative h-16 overflow-hidden rounded bg-wave">
      <WaveformOverview sections />
    </div>
    <div class="grid grid-cols-8 gap-1.5 pt-2">
      <HotCueBar playOnly />
    </div>
  </div>
</DeckShell>
