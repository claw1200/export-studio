import { createContext } from 'svelte'
import type { Analysis } from './analysis.svelte'
import type { Audition } from './audition.svelte'
import type { Backups } from './backups.svelte'
import type { Decks } from './decks.svelte'
import type { Keybinds } from './keybinds.svelte'
import type { Library } from './library.svelte'

export interface AppContext {
  library: Library
  decks: Decks
  keybinds: Keybinds
  analysis: Analysis
  backups: Backups
  audition: Audition
}

export const [getApp, setApp] = createContext<AppContext>()
