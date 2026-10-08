<script lang="ts">
  import { tick } from 'svelte'
  import { getApp } from '$lib/context'
  import type { GridDraft } from '$lib/beat'
  import { formatBpm } from '$lib/format'

  let { bpm }: { bpm: number } = $props()

  const { player } = getApp()

  const BPM_PER_PIXEL = 0.05
  const FINE_BPM_PER_PIXEL = 0.01
  const DRAG_THRESHOLD = 3

  let input: HTMLInputElement | undefined = $state()
  let typing = $state(false)
  let text = $state('')
  let drag: { y: number; bpm: number; base: GridDraft; moved: boolean } | null = null

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
    'tabular w-[76px] rounded border bg-transparent px-1 py-0.5 font-mono text-[15px] font-semibold text-fg',
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
    if (typing || e.button !== 0 || !player.grid.draft) return
    e.preventDefault()
    e.currentTarget.setPointerCapture(e.pointerId)
    drag = { y: e.clientY, bpm, base: player.grid.draft, moved: false }
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
