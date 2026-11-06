import { createContext } from 'svelte'
import type { Player } from '../player.svelte'

/**
 * The player a deck's controls act on. `<Deck>` sets it, so the waveform, transport, cue and loop
 * components inside work on whichever player that deck shows, however many decks are on screen.
 */
export const [getPlayer, setPlayer] = createContext<Player>()
