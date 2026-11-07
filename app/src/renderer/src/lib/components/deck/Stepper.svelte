<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte'

  /**
   * A value moved a step at a time, as the decks' BPM and key: − and + step it by 1, dragging the
   * value up or down steps it too, and clicking the value resets it.
   */
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
    /** Small text after the value, as the key's shift. */
    badge?: string
    /** On the outline, for a fixed width. */
    class?: string
  } = $props()

  /** Vertical travel, in pixels, per step when dragging the value. */
  const DRAG_STEP_PX = 8
  /** Where the drag started counting from, and whether it has stepped (so the release is not a click). */
  let drag: { pointer: number; y: number; moved: boolean } | null = null

  function dragDown(e: PointerEvent): void {
    if (e.button !== 0) return
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    drag = { pointer: e.pointerId, y: e.clientY, moved: false }
  }

  function dragMove(e: PointerEvent): void {
    if (!drag || drag.pointer !== e.pointerId) return
    const steps = Math.trunc((drag.y - e.clientY) / DRAG_STEP_PX)
    if (!steps) return
    drag.y -= steps * DRAG_STEP_PX
    drag.moved = true
    onstep(steps)
  }

  /** A release after a drag swallows the click that follows it, so dragging does not also reset. */
  function dragClick(): void {
    const moved = drag?.moved
    drag = null
    if (!moved) onreset()
  }
</script>

<div class={['flex h-6 items-center rounded border border-line-soft', disabled && 'opacity-40', className]}>
  <button
    class="grid h-full w-5 shrink-0 place-items-center text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none"
    {disabled}
    aria-label="{name} down"
    title="{name} down 1"
    onclick={() => onstep(-1)}><Minus class="size-3" /></button
  >
  <button
    class={[
      'tabular flex h-full min-w-[60px] flex-1 cursor-ns-resize touch-none items-center justify-center gap-1 px-1 font-mono text-[11.5px] font-semibold hover:bg-hover disabled:pointer-events-none',
      valueClass
    ]}
    style:color={color ?? undefined}
    {disabled}
    title="{title.replace(/\.?$/, '.')} Drag up or down to change it."
    onpointerdown={dragDown}
    onpointermove={dragMove}
    onpointercancel={() => (drag = null)}
    onclick={dragClick}
  >
    <span class="font-sans text-[9px] font-semibold tracking-wide text-faint uppercase">{name}</span>
    {value}
    {#if badge}<span class="text-[9.5px] text-faint">{badge}</span>{/if}
  </button>
  <button
    class="grid h-full w-5 shrink-0 place-items-center text-faint hover:bg-hover hover:text-fg disabled:pointer-events-none"
    {disabled}
    aria-label="{name} up"
    title="{name} up 1"
    onclick={() => onstep(1)}><Plus class="size-3" /></button
  >
</div>
