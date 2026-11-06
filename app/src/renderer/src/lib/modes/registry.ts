import PrepareMode from './PrepareMode.svelte'
import type { ModeDefinition } from './types'

/** Every mode, in the order the switcher lists them. The first is the default. */
export const MODES: readonly ModeDefinition[] = [
  { id: 'prepare', label: 'Prepare', decks: ['prepare'], component: PrepareMode }
]

export function modeById(id: string | null): ModeDefinition {
  return MODES.find((mode) => mode.id === id) ?? MODES[0]
}
