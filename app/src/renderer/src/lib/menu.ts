import type { Check } from '@lucide/svelte'

export interface MenuAction {
  kind?: 'item'
  label: string
  icon?: typeof Check
  shortcut?: string
  disabled?: boolean
  danger?: boolean
  checked?: boolean | 'mixed'
  swatch?: string
  title?: string
  keepOpen?: boolean
  onselect?: () => void
  items?: MenuEntry[]
}

export type MenuEntry = MenuAction | { kind: 'separator' } | { kind: 'header'; label: string }

export interface MenuRequest {
  x: number
  y: number
  /** A function is re-read while the menu is open, so items can reflect live state. */
  items: MenuEntry[] | (() => MenuEntry[])
  label: string
}

export function menuAt(e: MouseEvent, label: string, items: MenuRequest['items']): MenuRequest {
  return { x: e.clientX, y: e.clientY, items, label }
}

/** Drop separators that would sit at the edges or next to each other. */
export function tidy(items: (MenuEntry | false | null | undefined)[]): MenuEntry[] {
  const out: MenuEntry[] = []
  for (const item of items) {
    if (!item) continue
    if (item.kind === 'separator' && (!out.length || out.at(-1)?.kind === 'separator')) continue
    out.push(item)
  }
  while (out.at(-1)?.kind === 'separator') out.pop()
  return out
}
