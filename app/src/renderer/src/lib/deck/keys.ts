import { HOT_CUE_KINDS } from '../format'
import type { ActionHandler } from '../keybinds.svelte'
import { HOT_CUE_ACTIONS, type ActionId } from '../keymap'
import type { Player } from '../player.svelte'

type DeckHandlers = Partial<Record<ActionId, ActionHandler>>

/**
 * What a deck does while it is being played: transport, cue, loops, seeking, quantise and calling
 * cues the track already has. Nothing here changes the track's stored cues or beatgrid, so it is
 * all mix mode allows. `playPause` replaces the deck's own play/pause.
 */
export function playKeys(player: Player, playPause: () => void = () => player.toggle()): DeckHandlers {
  return {
    'deck.playPause': playPause,
    'deck.cue': { down: () => player.cueDown(), up: () => player.cueUp() },
    'deck.loopIn': () => player.setLoopIn(),
    'deck.loopOut': () => player.setLoopOut(),
    'deck.reloopExit': () => player.reloopExit(),
    'deck.beatLoop': () => player.beatLoop(4),
    'deck.loopHalve': () => player.resizeLoop(0.5),
    'deck.loopDouble': () => player.resizeLoop(2),
    'deck.quantise': () => player.setQuantise(!player.quantise),
    'deck.seekBack': () => player.seek(player.position - 2),
    'deck.seekForward': () => player.seek(player.position + 2),
    'deck.seekBackFar': () => player.seek(player.position - 10),
    'deck.seekForwardFar': () => player.seek(player.position + 10),
    ...hotCueKeys(player, false),
    'cue.previousMemory': () => player.callMemoryCue(-1),
    'cue.nextMemory': () => player.callMemoryCue(1)
  }
}

/** Everything a deck does in edit mode: playing it, and setting and deleting its cues. */
export function deckKeys(player: Player): DeckHandlers {
  return {
    ...playKeys(player),
    ...hotCueKeys(player, true),
    'cue.addMemory': () => void player.addMemoryCue(),
    'cue.deleteAtPlayhead': () => void player.deleteCueAtPlayhead()
  }
}

/** With `create`, an empty slot stores a hot cue; without, it does nothing. */
function hotCueKeys(player: Player, create: boolean): DeckHandlers {
  return Object.fromEntries(
    HOT_CUE_ACTIONS.map((id, i) => [
      id,
      { down: () => player.hotCueDown(HOT_CUE_KINDS[i], create), up: () => player.hotCueUp(HOT_CUE_KINDS[i]) }
    ])
  )
}
