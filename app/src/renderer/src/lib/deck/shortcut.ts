import { getApp } from '../context'
import { chordLabel, withShift, type ActionId } from '../keymap'
import { getDeckInfo } from './context'

/**
 * A deck control's shortcut hint, as " (⇧C)", for the end of a title: the action's first chord,
 * with Shift added on a shifted deck. Call while the component is set up; the hint it returns
 * follows the bindings as they change.
 */
export function useShortcut(): (id: ActionId) => string {
  const { keybinds } = getApp()
  const { shifted } = getDeckInfo()
  return (id) => {
    const chord = keybinds.bindings[id].map((c) => (shifted ? withShift(c) : c)).find((c) => c != null)
    return chord ? ` (${chordLabel(chord)})` : ''
  }
}

/** "deck A" or "the deck", for titles and labels. */
export function deckName(label: string | null): string {
  return label ? `deck ${label}` : 'the deck'
}
