<script lang="ts">
  import { bassLevel, bassSwap, crossfade, startBassSwap, type SwapRange } from '$lib/crossfade'

  /**
   * A fade from deck A to deck B: each deck's volume across the crossfader for `curve`, solid, and
   * its bass as the swap over `range` moves the LOW dials, dashed in the same colour. Drawn from
   * the same functions the mixer uses, so it is what will be heard.
   */
  let { curve, range }: { curve: number; range: SwapRange } = $props()

  const W = 200
  const H = 72
  const PAD = 5
  const STEPS = 100
  /** The bass lines top out at half height, so they stay clear of the volume lines at full level. */
  const BASS_HEIGHT = 0.5

  const positions = Array.from({ length: STEPS + 1 }, (_, i) => i / STEPS)

  function path(levels: number[]): string {
    return levels
      .map((level, i) => {
        const px = PAD + positions[i] * (W - PAD * 2)
        const py = PAD + (1 - level) * (H - PAD * 2)
        return `${i ? 'L' : 'M'} ${px.toFixed(1)} ${py.toFixed(1)}`
      })
      .join(' ')
  }

  const volume = $derived(positions.map((p) => crossfade(p, curve)))
  /** B's share of the bass at each step of a fade from A, as the mixer would swap it. */
  const shares = $derived.by(() => {
    let swap = startBassSwap(0)
    return positions.map((p) => (swap = bassSwap(swap, p, range)).share)
  })

  const lines = $derived([
    { d: path(volume.map(([a]) => a)), color: 'var(--color-fg)', bass: false },
    { d: path(volume.map(([, b]) => b)), color: 'var(--color-accent)', bass: false },
    { d: path(shares.map((s) => bassLevel(1 - s) * BASS_HEIGHT)), color: 'var(--color-fg)', bass: true },
    { d: path(shares.map((s) => bassLevel(s) * BASS_HEIGHT)), color: 'var(--color-accent)', bass: true }
  ])
</script>

<figure class="mb-3 flex flex-col items-center gap-1.5 py-2">
  <svg
    viewBox="0 0 {W} {H}"
    class="h-auto w-full max-w-[240px] rounded-md border border-line-soft bg-bg"
    role="img"
    aria-label="Deck A and deck B volume across the crossfader, and their bass as it swaps from {Math.round(
      range.start * 100
    )}% to {Math.round(range.end * 100)}% of the fade"
  >
    <line x1={W / 2} y1={PAD} x2={W / 2} y2={H - PAD} stroke="var(--color-line)" stroke-dasharray="2 3" />
    {#each lines as line, i (i)}
      <path
        d={line.d}
        fill="none"
        stroke={line.color}
        stroke-width={line.bass ? 1.25 : 1.75}
        stroke-dasharray={line.bass ? '3 2.5' : undefined}
        opacity={line.bass ? 0.8 : 1}
        stroke-linejoin="round"
      />
    {/each}
  </svg>
  <figcaption class="flex gap-4 text-[10.5px] text-faint">
    <span class="flex items-center gap-1.5"><span class="h-0.5 w-3 rounded bg-muted"></span>Volume</span>
    <span class="flex items-center gap-1.5"
      ><span class="w-3 border-t-[1.5px] border-dashed border-muted"></span>Bass</span
    >
  </figcaption>
</figure>
