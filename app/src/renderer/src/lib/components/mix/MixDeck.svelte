<script lang="ts">
  import { SquarePen } from '@lucide/svelte'
  import { setDeckInfo, setPlayer } from '$lib/deck/context'
  import type { Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import WaveformOverview from '../WaveformOverview.svelte'
  import DeckShell from '../deck/DeckShell.svelte'
  import HotCueBar from '../deck/HotCueBar.svelte'
  import LoopControls from '../deck/LoopControls.svelte'
  import MasterTempoButton from '../deck/MasterTempoButton.svelte'
  import TempoSteppers from '../deck/TempoSteppers.svelte'
  import TrackInfo from '../deck/TrackInfo.svelte'
  import Transport from '../deck/Transport.svelte'
  import { segmentGroup, segmentButton } from '../deck/segment'

  let {
    player,
    mixer,
    focused,
    onfocus,
    onedit
  }: {
    // Fixed for the component's life: key the deck on it to swap.
    player: Player
    mixer: Mixer
    focused: boolean
    onfocus: () => void
    onedit: (track: Track) => void
  } = $props()

  // svelte-ignore state_referenced_locally
  setPlayer(player)
  // svelte-ignore state_referenced_locally
  const label = mixer.label(player)
  setDeckInfo({ label, shifted: label === 'B' })

  const track = $derived(player.track)
  const isMaster = $derived(mixer.master === player)
  const following = $derived(mixer.following(player))
</script>

<DeckShell
  class={[
    'flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-surface transition-colors',
    focused ? 'border-accent/60' : 'border-line-soft'
  ]}
  waveformClass="h-[104px]"
  emptyMessage="Double-click or drag a track here to load it on deck {label}"
  onload={(loaded) => {
    onfocus()
    void player.load(loaded)
  }}
  {onfocus}
>
  {#snippet waveformOverlay()}
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
    <Transport playTitle="Play deck {label} on its own{following ? ', in time with the master' : ''}" />
    <LoopControls showBeatLoop={false} />

    <div class="ml-auto flex items-center gap-2">
      <div class={segmentGroup}>
        <button
          class={segmentButton(isMaster, 'solid')}
          disabled={!track}
          aria-pressed={isMaster}
          title={isMaster ? `Deck ${label} is the master` : `Make deck ${label} the master the other deck syncs to`}
          onclick={() => mixer.setMaster(player)}>MASTER</button
        >
      </div>
      <TempoSteppers />
      <MasterTempoButton />
    </div>
  </div>

  <div class="px-3 pt-2 pb-2.5">
    <div class="relative h-16 overflow-hidden rounded bg-wave">
      <WaveformOverview sections />
    </div>
    <HotCueBar playOnly class="pt-2" />
  </div>
</DeckShell>
