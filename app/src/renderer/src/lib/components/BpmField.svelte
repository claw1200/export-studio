<script lang="ts">
  import { tick } from 'svelte'
  import { getPlayer } from '$lib/deck/context'
  import type { TempoMap } from '$lib/tempo-map'
  import { formatBpm } from '$lib/format'

  /** `compact` sits inside a deck readout box, at the size of the text around it. */
  let { bpm, compact = false }: { bpm: number; compact?: boolean } = $props()

  const player = getPlayer()

  const BPM_PER_PIXEL = 0.05
  const FINE_BPM_PER_PIXEL = 0.01
  const DRAG_THRESHOLD = 3

  let input: HTMLInputElement | undefined = $state()
  let typing = $state(false)
  let text = $state('')
  let drag: { y: number; bpm: number; base: TempoMap; moved: boolean } | null = null

  async function startTyping(): Promise<void> {
    typing = true
    text = bpm ? bpm.toFixed(2) : ''
    await tick()
    input?.focus()
    input?.select()
  }

  function commit(): void {
    if (!typing) return
    typing = false
    const value = Number(text.trim().replace(',', '.'))
    if (value > 0) player.grid.setBpm(value)
  }

  function endDrag(): void {
    const ended = drag
    drag = null
    if (ended && !ended.moved) void startTyping()
  }
</script>

<input
  bind:this={input}
  class={[
    'tabular rounded border bg-transparent font-mono font-semibold text-fg',
    compact ? 'h-5 w-[50px] min-w-0 px-0.5 text-[11.5px]' : 'w-[76px] px-1 py-0.5 text-[15px]',
    typing
      ? 'cursor-text border-accent/60 bg-bg'
      : 'cursor-ns-resize touch-none border-transparent select-none hover:border-line-soft'
  ]}
  inputmode="decimal"
  aria-label="Beatgrid BPM. Drag up or down to change, or click to type."
  title="Drag up or down to change the BPM (Shift for fine steps), or click to type"
  readonly={!typing}
  disabled={player.editing}
  value={typing ? text : formatBpm(bpm) || '–'}
  oninput={(e) => (text = e.currentTarget.value)}
  onpointerdown={(e) => {
    if (typing || e.button !== 0 || !player.grid.map) return
    e.preventDefault()
    e.currentTarget.setPointerCapture(e.pointerId)
    drag = { y: e.clientY, bpm, base: player.grid.map, moved: false }
  }}
  onpointermove={(e) => {
    if (!drag) return
    const dy = drag.y - e.clientY
    if (!drag.moved && Math.abs(dy) < DRAG_THRESHOLD) return
    drag.moved = true
    player.grid.setBpm(drag.bpm + dy * (e.shiftKey ? FINE_BPM_PER_PIXEL : BPM_PER_PIXEL), drag.base)
  }}
  onmousedown={(e) => {
    if (!typing) e.preventDefault()
  }}
  onpointerup={endDrag}
  onpointercancel={() => (drag = null)}
  onkeydown={(e) => {
    if (!typing) return
    if (e.key === 'Enter') {
      e.preventDefault()
      commit()
      input?.blur()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      typing = false
      input?.blur()
    }
  }}
  onblur={(e) => {
    commit()
    e.currentTarget.setSelectionRange(0, 0)
  }}
/>
