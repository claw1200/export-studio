import { createContext } from 'svelte'
import type { Library } from './library.svelte'
import type { Player } from './player.svelte'

export interface AppContext {
  library: Library
  player: Player
}

export const [getApp, setApp] = createContext<AppContext>()
