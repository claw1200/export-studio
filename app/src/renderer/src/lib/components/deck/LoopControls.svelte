<script lang="ts">
  import { getApp } from '$lib/context'

  const { player } = getApp()

  const noTrack = $derived(!player.track || !!player.audioError)
  const loopButton = (lit: boolean): string[] => [
    'flex h-7 items-center rounded-md border px-2 text-[10.5px] font-bold tracking-wide transition disabled:opacity-30',
    lit
      ? 'border-transparent bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
      : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
  ]
</script>

<div class="ml-auto flex items-center gap-3">
  <div class="flex items-center gap-1" role="group" aria-label="Loop">
    <button class={loopButton(false)} disabled={noTrack} onclick={() => player.setLoopIn()} title="Loop in">
      IN
    </button>
    <button class={loopButton(false)} disabled={noTrack} onclick={() => player.setLoopOut()} title="Loop out">
      OUT
    </button>
    <button
      class={loopButton(!!player.loop?.active)}
      disabled={!player.loop}
      onclick={() => player.reloopExit()}
      aria-pressed={!!player.loop?.active}
      title={player.loop?.active ? 'Exit loop' : 'Reloop'}
    >
      {player.loop?.active ? 'EXIT' : 'RELOOP'}
    </button>
    <button class={loopButton(false)} disabled={noTrack} onclick={() => player.beatLoop(4)} title="4-beat loop">
      4 BEAT
    </button>
    <button
      class={loopButton(false)}
      disabled={!player.loop}
      onclick={() => player.resizeLoop(0.5)}
      aria-label="Halve loop"
      title="Halve loop"
    >
      ½
    </button>
    <button
      class={loopButton(false)}
      disabled={!player.loop}
      onclick={() => player.resizeLoop(2)}
      aria-label="Double loop"
      title="Double loop"
    >
      ×2
    </button>
  </div>
  <button
    class={[
      'flex h-7 items-center rounded-md border px-2 text-[11px] font-semibold transition',
      player.quantise
        ? 'border-transparent bg-accent-soft text-accent'
        : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
    ]}
    onclick={() => player.setQuantise(!player.quantise)}
    aria-pressed={player.quantise}
    title={player.quantise ? 'Quantise on. Scrubbing and new hot cues snap to the beat.' : 'Quantise off'}
  >
    Quantise
  </button>
</div>
