import { getApp } from '../context'
import { shortcutHint, type ActionId } from '../keymap'
import { getDeckInfo } from './context'

/** Shortcut hints for a deck's titles, Shift-ed on a shifted deck. Call during component setup. */
export function useShortcut(): (id: ActionId) => string {
  const { keybinds } = getApp()
  const { shifted } = getDeckInfo()
  return (id) => shortcutHint(keybinds.bindings[id], shifted)
}
