<script lang="ts" module>
  export type Reading = { text: string; meaning: string; stepSize: string; onstep: (steps: number) => void }
</script>

<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { layerModifierName } from '$lib/keymap'
  import { layerModifierHeld } from '$lib/layerModifierHeld.svelte'

  let {
    name,
    reading,
    modifiedReading,
    edited,
    title,
    disabled = false,
    onreset,
    class: className = ''
  }: {
    name: string
    reading: Reading
    modifiedReading: Reading
    edited: boolean
    title: string
    disabled?: boolean
    onreset: () => void
    class?: string
  } = $props()

  const DRAG_PX_PER_STEP = 8
  const asSentence = (text: string): string => text.replace(/\.?$/, '.')
  const { keybinds } = getApp()
  const shown = $derived(layerModifierHeld(keybinds.layerModifier) ? modifiedReading : reading)
  const tooltip = $derived(
    `${name}: ${asSentence(title)} Drag up or down to change it. Hold ${layerModifierName(keybinds.layerModifier)} to see and change ${modifiedReading.meaning}.`
  )
  let drag: { pointer: number; y: number; moved: boolean } | null = null

  function dragStart(e: PointerEvent): void {
    if (e.button !== 0) return
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    drag = { pointer: e.pointerId, y: e.clientY, moved: false }
  }

  function dragMove(e: PointerEvent): void {
    if (!drag || drag.pointer !== e.pointerId) return
    const steps = Math.trunc((drag.y - e.clientY) / DRAG_PX_PER_STEP)
    if (!steps) return
    drag.y -= steps * DRAG_PX_PER_STEP
    drag.moved = true
    shown.onstep(steps)
  }

  function resetUnlessDragged(): void {
    const dragged = drag?.moved
    drag = null
    if (!dragged) onreset()
  }
</script>

{#snippet stepButton(by: -1 | 1)}
  {@const direction = by < 0 ? 'down' : 'up'}
  <button
    class="grid h-full w-5 shrink-0 place-items-center text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none"
    {disabled}
    aria-label="{name} {direction}"
    title="{name} {direction} {shown.stepSize}"
    onclick={() => shown.onstep(by)}
  >
    {#if by < 0}<Minus class="size-3" />{:else}<Plus class="size-3" />{/if}
  </button>
{/snippet}

<div class={['flex h-6 items-center rounded border border-line-soft', disabled && 'opacity-40', className]}>
  {@render stepButton(-1)}
  <button
    class={[
      'tabular flex h-full min-w-0 flex-1 cursor-ns-resize touch-none items-center justify-center gap-1 px-1 font-mono text-[11.5px] font-semibold hover:bg-hover disabled:pointer-events-none',
      edited ? 'text-accent' : 'text-fg'
    ]}
    {disabled}
    aria-label="{name} {reading.text}, {modifiedReading.text}"
    title={tooltip}
    onpointerdown={dragStart}
    onpointermove={dragMove}
    onpointercancel={() => (drag = null)}
    onclick={resetUnlessDragged}
  >
    <span class="font-sans text-[9px] font-semibold tracking-wide text-faint uppercase" aria-hidden="true">{name}</span>
    {shown.text}
  </button>
  {@render stepButton(1)}
</div>
