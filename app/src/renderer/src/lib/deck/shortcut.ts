import { getApp } from '../context'
import { shortcutHint, type ActionId } from '../keymap'
import { getDeckInfo } from './context'

/** Shortcut hints for a deck's titles, with the layer modifier on deck B. Call during component setup. */
export function useShortcut(): (id: ActionId) => string {
  const { keybinds } = getApp()
  const { modifiedKeys } = getDeckInfo()
  return (id) => shortcutHint(keybinds.bindings[id], modifiedKeys ? keybinds.layerModifier : null)
}
