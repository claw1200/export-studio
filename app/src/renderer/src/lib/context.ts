import { createContext } from 'svelte'
import type { Analysis } from './analysis.svelte'
import type { Keybinds } from './keybinds.svelte'
import type { Library } from './library.svelte'
import type { Player } from './player.svelte'

export interface AppContext {
  library: Library
  player: Player
  keybinds: Keybinds
  analysis: Analysis
}

export const [getApp, setApp] = createContext<AppContext>()
