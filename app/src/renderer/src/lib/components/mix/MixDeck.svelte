<script lang="ts">
  import { LoaderCircle, Minus, Music, Pause, Play, Plus, SquarePen } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { setPlayer } from '$lib/deck/context'
  import { TrackDrop } from '$lib/deck/drop.svelte'
  import { getApp } from '$lib/context'
  import { chordLabel, withShift, type ActionId } from '$lib/keymap'
  import { formatBpm, formatClock, keyColor, pitchSemitones } from '$lib/format'
  import { heardKey } from '$lib/mix/harmonic'
  import { ownBpm, playingBpm, type Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import WaveformDetail from '../WaveformDetail.svelte'
  import WaveformOverview from '../WaveformOverview.svelte'

  /**
   * One side of the mixer: the track, where its playhead is, the scrolling waveform and an overview
   * split into the sections between cues. Set the playhead by clicking the overview or dragging the
   * scrolling waveform; the mixer's play starts both, or the deck's own play and cue start it alone.
   * Loop in, out and reloop/exit work as on a CDJ. They share a row under the track with its tempo
   * and key, each moved a step at a time, and the switches that make it the master and hold its key;
   * a narrow deck wraps the tempo side onto a second line.
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

  const { library, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  const drop = new TrackDrop(library, (track) => {
    onfocus()
    void player.load(track)
  })

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
  const noAudio = $derived(!track || !!player.audioError)
  const cueLit = $derived(player.cueHeld || (!player.playing && !!track && player.atCue))
  /** Deck B's shortcuts are deck A's with Shift added: see `MixMode`. */
  const shifted = $derived(player === mixer.b)
  const shortcut = (id: ActionId): string => {
    const chord = keybinds.bindings[id].map((c) => (shifted ? withShift(c) : c)).find((c) => c != null)
    return chord ? ` (${chordLabel(chord)})` : ''
  }
  const cueTitle = $derived(
    (player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`) + shortcut('deck.cue')
  )
  const loopButton = (lit: boolean): string[] => [
    'h-6 rounded px-1.5 text-[10.5px] font-bold tracking-wide transition disabled:opacity-30',
    lit
      ? 'bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
      : 'border border-line text-faint hover:bg-hover hover:text-muted'
  ]
  const signed = (n: number): string => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '')

  /** Vertical travel, in pixels, per step when dragging a stepper's value. */
  const DRAG_STEP_PX = 8
  /** Where the drag started counting from, and whether it has stepped (so the release is not a click). */
  let drag: { pointer: number; y: number; moved: boolean } | null = null

  function dragDown(e: PointerEvent): void {
    if (e.button !== 0) return
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    drag = { pointer: e.pointerId, y: e.clientY, moved: false }
  }

  function dragMove(e: PointerEvent, onstep: (step: number) => void): void {
    if (!drag || drag.pointer !== e.pointerId) return
    const steps = Math.trunc((drag.y - e.clientY) / DRAG_STEP_PX)
    if (!steps) return
    drag.y -= steps * DRAG_STEP_PX
    drag.moved = true
    onstep(steps)
  }

  /** A release after a drag swallows the click that follows it, so dragging does not also reset. */
  function dragClick(onreset: () => void): void {
    const moved = drag?.moved
    drag = null
    if (!moved) onreset()
  }
</script>

{#snippet stepper(
  name: string,
  value: string,
  valueClass: string | null,
  color: string | null,
  title: string,
  disabled: boolean,
  onstep: (step: number) => void,
  onreset: () => void,
  badge: string = ''
)}
  <div class={['flex h-6 items-center rounded border border-line-soft', disabled && 'opacity-40']}>
    <button
      class="grid h-full w-5 place-items-center text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none"
      {disabled}
      aria-label="{name} down"
      title="{name} down 1"
      onclick={() => onstep(-1)}><Minus class="size-3" /></button
    >
    <button
      class={[
        'tabular flex h-full min-w-[60px] cursor-ns-resize touch-none items-center justify-center gap-1 px-1 font-mono text-[11.5px] font-semibold hover:bg-hover disabled:pointer-events-none',
        valueClass
      ]}
      style:color={color ?? undefined}
      {disabled}
      title="{title.replace(/\.?$/, '.')} Drag up or down to change it."
      onpointerdown={dragDown}
      onpointermove={(e) => dragMove(e, onstep)}
      onpointercancel={() => (drag = null)}
      onclick={() => dragClick(onreset)}
    >
      <span class="text-[9px] font-sans font-semibold tracking-wide text-faint uppercase">{name}</span>
      {value}
      {#if badge}<span class="text-[9.5px] text-faint">{badge}</span>{/if}
    </button>
    <button
      class="grid h-full w-5 place-items-center text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none"
      {disabled}
      aria-label="{name} up"
      title="{name} up 1"
      onclick={() => onstep(1)}><Plus class="size-3" /></button
    >
  </div>
{/snippet}

{#snippet placeholder()}
  <div class="grid h-full place-items-center px-4 text-center text-[12px] text-faint">
    {#if player.loading}
      <LoaderCircle class="size-4 animate-spin" />
    {:else if track}
      No analysis data for this track
    {:else}
      Double-click or drag a track here to load it on deck {label}
    {/if}
  </div>
{/snippet}

<section
  class={[
    'relative flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-surface transition-colors',
    focused ? 'border-accent/60' : 'border-line-soft'
  ]}
  onpointerdowncapture={onfocus}
  aria-label="Deck {label}"
  ondragenter={drop.ondragenter}
  ondragover={drop.ondragover}
  ondragleave={drop.ondragleave}
  ondrop={drop.ondrop}
>
  {#if drop.over}
    <div
      class="pointer-events-none absolute inset-0 z-30 grid place-items-center rounded-[inherit] border-2 border-accent bg-accent-soft"
    >
      <span class="rounded bg-bg/80 px-2 py-1 text-[12px] font-semibold text-accent">Load on deck {label}</span>
    </div>
  {/if}
  <div class="relative h-[104px] border-b border-line-soft bg-[oklch(0.12_0.005_260)]">
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      {@render placeholder()}
    {/if}
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
  </div>

  <div class="flex items-center gap-2.5 px-3 pt-2.5">
    <div class="size-10 shrink-0 overflow-hidden rounded-md bg-raised">
      {#if track?.hasArtwork}
        <img src={artworkUrl(track.id, 'm')} alt="" class="size-full object-cover" draggable="false" />
      {:else}
        <div class="grid size-full place-items-center text-faint"><Music class="size-4" /></div>
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      {#if track}
        <div class="truncate text-[13.5px] font-semibold text-fg" title={track.title}>{track.title}</div>
        <div class="tabular mt-0.5 flex items-center gap-1.5 text-[11px]">
          <span class="truncate text-muted">{track.artist || 'Unknown artist'}</span>
        </div>
      {:else}
        <div class="text-[13px] text-faint">No track loaded</div>
      {/if}
    </div>
    <div class="tabular shrink-0 text-right font-mono">
      <div class="text-[14px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
      <div class="text-[11px] text-faint">−{formatClock(Math.max(0, player.duration - player.position) * 1000)}</div>
    </div>
  </div>

  <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-3 pt-2">
    <button
      class={[
        'grid size-7 place-items-center rounded-full transition disabled:opacity-30',
        player.playing ? 'bg-accent text-bg hover:brightness-110' : 'bg-raised text-fg hover:bg-hover'
      ]}
      disabled={noAudio}
      onclick={() => mixer.toggleDeck(player)}
      aria-label={player.playing ? `Pause deck ${label}` : `Play deck ${label}`}
      title={(player.playing
        ? `Pause deck ${label}`
        : `Play deck ${label} on its own${following ? ', in time with the master' : ''}`) + shortcut('deck.playPause')}
    >
      {#if player.playing}
        <Pause class="size-3.5 fill-current" />
      {:else}
        <Play class="ml-px size-3.5 fill-current" />
      {/if}
    </button>
    <button
      class={[
        'h-6 w-11 touch-none rounded-full border text-[10.5px] font-bold tracking-wide transition select-none disabled:opacity-30',
        cueLit
          ? 'border-transparent bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
          : 'border-cue/60 text-cue hover:bg-cue/15'
      ]}
      disabled={noAudio}
      onpointerdown={(e) => {
        if (e.button !== 0) return
        e.currentTarget.setPointerCapture(e.pointerId)
        player.cueDown()
      }}
      onpointerup={() => player.cueUp()}
      onpointercancel={() => player.cueUp()}
      onlostpointercapture={() => player.cueUp()}
      aria-label="Cue deck {label}"
      title={cueTitle}>CUE</button
    >

    <div class="flex items-center gap-1" role="group" aria-label="Deck {label} loop">
      <button
        class={loopButton(false)}
        disabled={noAudio}
        onclick={() => player.setLoopIn()}
        title="Loop in{shortcut('deck.loopIn')}">IN</button
      >
      <button
        class={loopButton(false)}
        disabled={noAudio}
        onclick={() => player.setLoopOut()}
        title="Loop out{shortcut('deck.loopOut')}">OUT</button
      >
      <button
        class={['inline-grid place-items-center', ...loopButton(!!player.loop?.active)]}
        disabled={!player.loop}
        onclick={() => player.reloopExit()}
        aria-pressed={!!player.loop?.active}
        title="{player.loop?.active ? 'Exit the loop' : 'Back into the loop'}{shortcut('deck.reloopExit')}"
      >
        <!-- Both labels share one cell, so the button is as wide as the longer and keeps its width. -->
        <span class={['col-start-1 row-start-1', !player.loop?.active && 'invisible']}>EXIT</span>
        <span class={['col-start-1 row-start-1', player.loop?.active && 'invisible']}>RELOOP</span>
      </button>
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
      {@render stepper(
        'BPM',
        track ? formatBpm(bpm) : '—',
        rate !== 1 ? 'text-accent' : 'text-fg',
        null,
        bpmTitle,
        !track || following || !(ownBpm(player) > 0),
        (step) => mixer.stepBpm(player, step),
        () => mixer.resetTempo(player)
      )}
      {@render stepper(
        'Key',
        key || '—',
        null,
        key ? keyColor(key) : null,
        keyTitle,
        !track,
        (step) => mixer.stepKey(player, step),
        () => mixer.resetKey(player),
        signed(keyShift)
      )}
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
    <div class="relative h-16 overflow-hidden rounded bg-[oklch(0.12_0.005_260)]">
      <WaveformOverview sections />
    </div>
  </div>
</section>
