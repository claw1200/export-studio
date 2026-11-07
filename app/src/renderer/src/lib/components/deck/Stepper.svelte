<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte'

  /** − and + step by 1, dragging the value steps too, and clicking the value resets it. */
  let {
    name,
    value,
    valueClass = null,
    color = null,
    title,
    disabled = false,
    onstep,
    onreset,
    badge = '',
    class: className = ''
  }: {
    name: string
    value: string
    valueClass?: string | null
    color?: string | null
    title: string
    disabled?: boolean
    onstep: (step: number) => void
    onreset: () => void
    badge?: string
    class?: string
  } = $props()

  const DRAG_PX_PER_STEP = 8
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
    onstep(steps)
  }

  /** The click that ends a drag must not also reset. */
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
    title="{name} {direction} 1"
    onclick={() => onstep(by)}
  >
    {#if by < 0}<Minus class="size-3" />{:else}<Plus class="size-3" />{/if}
  </button>
{/snippet}

<div class={['flex h-6 items-center rounded border border-line-soft', disabled && 'opacity-40', className]}>
  {@render stepButton(-1)}
  <button
    class={[
      'tabular flex h-full min-w-[60px] flex-1 cursor-ns-resize touch-none items-center justify-center gap-1 px-1 font-mono text-[11.5px] font-semibold hover:bg-hover disabled:pointer-events-none',
      valueClass
    ]}
    style:color={color ?? undefined}
    {disabled}
    title="{title.replace(/\.?$/, '.')} Drag up or down to change it."
    onpointerdown={dragStart}
    onpointermove={dragMove}
    onpointercancel={() => (drag = null)}
    onclick={resetUnlessDragged}
  >
    <span class="font-sans text-[9px] font-semibold tracking-wide text-faint uppercase">{name}</span>
    {value}
    {#if badge}<span class="text-[9.5px] text-faint">{badge}</span>{/if}
  </button>
  {@render stepButton(1)}
</div>
