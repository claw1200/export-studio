<script lang="ts">
  import { bassLevel } from '$lib/crossfade'

  /**
   * A deck's isolator LOW knob, drawn to show what the bass swap is doing; it is not a control.
   * Like a mixer's, the centre is unity and fully left is a kill. The swap never boosts, so the
   * knob only ever sits in the left half.
   */
  let { label, value, active }: { label: string; value: number; active: boolean } = $props()

  /** Knob travel either side of 12 o'clock, in degrees. */
  const SWEEP = 135
  const R = 13
  const C = 16

  /** A point on the knob's ring at `degrees` clockwise from 12 o'clock. */
  function at(degrees: number, radius = R): [number, number] {
    const rad = (degrees * Math.PI) / 180
    return [C + radius * Math.sin(rad), C - radius * Math.cos(rad)]
  }

  function arc(from: number, to: number): string {
    const [x0, y0] = at(from)
    const [x1, y1] = at(to)
    const large = Math.abs(to - from) > 180 ? 1 : 0
    const sweep = to > from ? 1 : 0
    return `M ${x0} ${y0} A ${R} ${R} 0 ${large} ${sweep} ${x1} ${y1}`
  }

  const position = $derived(Math.min(1, Math.max(0, value)))
  const angle = $derived(-SWEEP * (1 - position))
  const db = $derived.by(() => {
    const level = bassLevel(position)
    if (level <= 0) return '−∞'
    const v = Math.round(20 * Math.log10(level))
    return v === 0 ? '0' : `−${-v}`
  })
  const pointer = $derived([at(angle, 4), at(angle, R - 3)])
</script>

<div
  class={['flex flex-col items-center gap-0.5 transition-opacity', !active && 'opacity-40']}
  role="img"
  aria-label="Deck {label} low: {db} dB"
  title={active ? `Deck ${label} LOW at ${db} dB` : `Deck ${label} LOW: swap bass is off`}
>
  <svg viewBox="0 0 32 32" class="size-8" aria-hidden="true">
    <path d={arc(-SWEEP, SWEEP)} fill="none" stroke="var(--color-raised)" stroke-width="2.5" stroke-linecap="round" />
    {#if position < 1}
      <path d={arc(angle, 0)} fill="none" stroke="var(--color-accent)" stroke-width="2.5" stroke-linecap="round" />
    {/if}
    <line x1={C} y1={C - R - 2} x2={C} y2={C - R + 1} stroke="var(--color-line)" stroke-width="1" />
    <circle cx={C} cy={C} r={R - 3.5} fill="var(--color-surface)" stroke="var(--color-line)" stroke-width="1" />
    <line
      x1={pointer[0][0]}
      y1={pointer[0][1]}
      x2={pointer[1][0]}
      y2={pointer[1][1]}
      stroke="var(--color-fg)"
      stroke-width="1.75"
      stroke-linecap="round"
    />
  </svg>
  <span class="text-[9px] font-semibold tracking-wide text-faint uppercase">Low {label}</span>
  <span class="tabular font-mono text-[10px] text-muted">{db}</span>
</div>
