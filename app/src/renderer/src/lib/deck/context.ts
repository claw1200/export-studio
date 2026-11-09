import { createContext } from 'svelte'
import type { Player } from '../player.svelte'

/**
 * The player a deck's controls act on. `<Deck>` sets it, so the waveform, transport, cue and loop
 * components inside work on whichever player that deck shows, however many decks are on screen.
 */
export const [getPlayer, setPlayer] = createContext<Player>()

export interface DeckInfo {
  /** Mix mode's "A" or "B"; edit mode's single deck has none. */
  label: string | null
  /** Deck B in mix mode takes deck A's shortcuts with Shift added. */
  shifted: boolean
}

export const [getDeckInfo, setDeckInfo] = createContext<DeckInfo>()

export function deckName({ label }: DeckInfo): string {
  return label ? `deck ${label}` : 'the deck'
}

export function onNearestBeat(quantise: boolean): string {
  return quantise ? ', on the nearest beat' : ''
}
