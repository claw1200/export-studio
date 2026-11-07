import { createContext } from 'svelte'
import type { Player } from '../player.svelte'

/**
 * The player a deck's controls act on. `<Deck>` sets it, so the waveform, transport, cue and loop
 * components inside work on whichever player that deck shows, however many decks are on screen.
 */
export const [getPlayer, setPlayer] = createContext<Player>()

/** How a deck is named and reached from the keyboard, for the controls inside it to say so. */
export interface DeckInfo {
  /** "A" or "B" in mix mode; null for the one deck of edit mode. */
  label: string | null
  /** Its shortcuts are the usual ones with Shift added, as deck B's in mix mode. */
  shifted: boolean
}

export const [getDeckInfo, setDeckInfo] = createContext<DeckInfo>()
