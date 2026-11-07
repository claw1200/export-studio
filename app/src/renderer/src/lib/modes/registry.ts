import MixMode from './MixMode.svelte'
import EditMode from './EditMode.svelte'
import type { ModeDefinition } from './types'

/** Every mode, in the order the switcher lists them. The first is the default. */
export const MODES: readonly ModeDefinition[] = [
  { id: 'edit', label: 'Edit', decks: ['edit'], inspector: true, component: EditMode },
  { id: 'mix', label: 'Mix', decks: ['mixA', 'mixB'], inspector: false, component: MixMode }
]

export function modeById(id: string | null): ModeDefinition {
  return MODES.find((mode) => mode.id === id) ?? MODES[0]
}
