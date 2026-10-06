import { createContext } from 'svelte'
import type { Keybinds } from './keybinds.svelte'
import type { Library } from './library.svelte'
import type { Player } from './player.svelte'

export interface AppContext {
  library: Library
  player: Player
  keybinds: Keybinds
}

export const [getApp, setApp] = createContext<AppContext>()
