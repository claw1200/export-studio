/**
 * Buttons joined into one control: related actions read as a group by their shared outline, so the
 * deck's rows need no dividers between groups, only space.
 */
export const segment = 'flex h-6 shrink-0 divide-x divide-line overflow-hidden rounded border border-line'

/** How a lit button in a segment is coloured: the cue's amber for loops, the accent for switches. */
type Tone = 'cue' | 'accent' | 'flag'

const LIT: Record<Tone, string> = {
  cue: 'bg-cue text-bg',
  accent: 'bg-accent-soft text-accent',
  flag: 'bg-flag/15 text-flag'
}

/** One button in a segment. `tone` without `lit` only tints the label, as for the tempo flag. */
export function segmentButton(lit = false, tone: Tone = 'cue'): string[] {
  return [
    'grid min-w-6 place-items-center px-1.5 text-[10.5px] font-bold tracking-wide transition-colors disabled:pointer-events-none disabled:opacity-30',
    lit ? LIT[tone] : tone === 'flag' ? 'text-flag hover:bg-flag/15' : 'text-faint hover:bg-hover hover:text-muted'
  ]
}
