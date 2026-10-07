<script lang="ts">
  import { Check, ChevronRight, Minus } from '@lucide/svelte'
  import type { MenuAction, MenuEntry } from '$lib/menu'
  import MenuPanel from './MenuPanel.svelte'

  let {
    items,
    x,
    y,
    flipX,
    label,
    autofocus = false,
    onclose,
    onback
  }: {
    items: MenuEntry[]
    x: number
    y: number
    flipX: number
    label: string
    autofocus?: boolean
    onclose: () => void
    onback?: () => void
  } = $props()

  const MARGIN = 8
  const OPEN_DELAY = 120

  let panel = $state<HTMLElement>()
  let open = $state<{ index: number; x: number; y: number; flipX: number; autofocus: boolean } | null>(null)
  let hoverTimer: ReturnType<typeof setTimeout> | undefined

  function place(node: HTMLElement): void {
    const rect = node.getBoundingClientRect()
    const left = x + rect.width > window.innerWidth - MARGIN ? flipX - rect.width : x
    const top = y + rect.height > window.innerHeight - MARGIN ? window.innerHeight - MARGIN - rect.height : y
    node.style.left = `${Math.max(MARGIN, left)}px`
    node.style.top = `${Math.max(MARGIN, top)}px`
    if (autofocus) ownItems(node)[0]?.focus({ preventScroll: true })
    else node.focus({ preventScroll: true })
  }

  function ownItems(panel: HTMLElement): HTMLButtonElement[] {
    return [...panel.querySelectorAll<HTMLButtonElement>(':scope > button[data-item]:not(:disabled)')]
  }

  function isAction(item: MenuEntry): item is MenuAction {
    return item.kind === undefined || item.kind === 'item'
  }

  function openSub(index: number, button: HTMLElement, focus = false): void {
    clearTimeout(hoverTimer)
    const rect = button.getBoundingClientRect()
    open = { index, x: rect.right + 2, y: rect.top - 5, flipX: rect.left - 2, autofocus: focus }
  }

  function hover(index: number, item: MenuAction, button: HTMLElement): void {
    clearTimeout(hoverTimer)
    if (open?.index === index) return
    hoverTimer = setTimeout(() => {
      if (item.items?.length && !item.disabled) openSub(index, button)
      else open = null
    }, OPEN_DELAY)
  }

  function choose(index: number, item: MenuAction, button: HTMLElement): void {
    if (item.disabled) return
    if (item.items) {
      openSub(index, button, true)
      return
    }
    if (!item.keepOpen) onclose()
    item.onselect?.()
  }

  function closeSub(): void {
    const index = open?.index
    open = null
    if (index == null) return
    panel?.querySelector<HTMLButtonElement>(`:scope > button[data-index="${index}"]`)?.focus({ preventScroll: true })
  }

  function onkeydown(e: KeyboardEvent): void {
    const node = e.currentTarget as HTMLElement
    const buttons = ownItems(node)
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const step = e.key === 'ArrowDown' ? 1 : -1
      const next =
        current < 0 ? (step > 0 ? 0 : buttons.length - 1) : (current + step + buttons.length) % buttons.length
      buttons[next]?.focus({ preventScroll: false })
    } else if (e.key === 'ArrowRight' && current >= 0) {
      const index = Number(buttons[current].dataset.index)
      const item = items[index]
      if (!isAction(item) || !item.items?.length) return
      openSub(index, buttons[current], true)
    } else if ((e.key === 'ArrowLeft' || e.key === 'Escape') && onback) {
      onback()
    } else if (e.key === 'Escape') {
      onclose()
    } else {
      return
    }
    e.preventDefault()
    e.stopPropagation()
  }
</script>

<div
  bind:this={panel}
  {@attach place}
  class="fixed z-50 max-h-[calc(100vh-16px)] min-w-44 overflow-y-auto rounded-lg border border-line bg-surface p-1 shadow-xl shadow-black/40 focus:outline-none"
  style:left="{x}px"
  style:top="{y}px"
  role="menu"
  tabindex="-1"
  aria-label={label}
  {onkeydown}
  oncontextmenu={(e) => {
    e.preventDefault()
    e.stopPropagation()
  }}
>
  {#each items as item, index (index)}
    {#if item.kind === 'separator'}
      <div class="mx-2 my-1 border-t border-line-soft" role="separator"></div>
    {:else if item.kind === 'header'}
      <div class="truncate px-2 pt-1.5 pb-1 text-[11px] font-medium text-faint">{item.label}</div>
    {:else}
      <button
        type="button"
        data-item
        data-index={index}
        class={[
          'flex w-full max-w-72 items-center gap-2 rounded-md px-2 py-1 text-left text-[12.5px] focus:outline-none disabled:cursor-default disabled:text-faint',
          item.danger ? 'text-danger' : 'text-fg',
          open?.index === index ? 'bg-hover' : 'enabled:hover:bg-hover enabled:focus:bg-hover'
        ]}
        role={item.checked !== undefined ? 'menuitemcheckbox' : 'menuitem'}
        aria-checked={item.checked === 'mixed' ? 'mixed' : item.checked}
        aria-haspopup={item.items ? 'menu' : undefined}
        aria-expanded={item.items ? open?.index === index : undefined}
        disabled={item.disabled}
        title={item.title}
        onpointerenter={(e) => hover(index, item, e.currentTarget)}
        onclick={(e) => choose(index, item, e.currentTarget)}
      >
        <span class="grid size-3.5 shrink-0 place-items-center text-accent">
          {#if item.swatch}
            <span class="size-2.5 rounded-full" style:background={item.swatch}></span>
          {:else if item.checked === true}
            <Check class="size-3.5" strokeWidth={2.5} />
          {:else if item.checked === 'mixed'}
            <Minus class="size-3.5" strokeWidth={2.5} />
          {:else if item.icon}
            <item.icon class={['size-3.5', item.danger ? 'text-danger' : 'text-faint']} />
          {/if}
        </span>
        <span class="min-w-0 flex-1 truncate">{item.label}</span>
        {#if item.items}
          <ChevronRight class="size-3.5 shrink-0 text-faint" />
        {:else if item.swatch && item.checked === true}
          <Check class="size-3.5 shrink-0 text-accent" strokeWidth={2.5} />
        {:else if item.shortcut}
          <span class="shrink-0 pl-4 text-[11px] text-faint">{item.shortcut}</span>
        {/if}
      </button>
    {/if}
  {/each}

  {#if open}
    {@const parent = items[open.index]}
    {#if isAction(parent) && parent.items}
      <MenuPanel
        items={parent.items}
        x={open.x}
        y={open.y}
        flipX={open.flipX}
        label={parent.label}
        autofocus={open.autofocus}
        {onclose}
        onback={closeSub}
      />
    {/if}
  {/if}
</div>
