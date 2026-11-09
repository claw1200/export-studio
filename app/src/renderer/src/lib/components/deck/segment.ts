export const segmentGroup = 'flex h-6 shrink-0 divide-x divide-line overflow-hidden rounded border border-line'

type Tone = 'cue' | 'accent' | 'solid' | 'flag'

const LIT: Record<Tone, string> = {
  cue: 'bg-cue text-bg',
  accent: 'bg-accent-soft text-accent',
  solid: 'bg-accent text-bg',
  flag: 'bg-flag/15 text-flag'
}

const UNLIT = 'text-faint hover:bg-hover hover:text-muted'
const UNLIT_FLAG = 'text-flag hover:bg-flag/15'

export function segmentButton(lit = false, tone: Tone = 'cue'): string[] {
  return [
    'grid min-w-6 place-items-center px-1.5 text-[10.5px] font-bold tracking-wide transition-colors disabled:pointer-events-none disabled:opacity-30',
    lit ? LIT[tone] : tone === 'flag' ? UNLIT_FLAG : UNLIT
  ]
}
