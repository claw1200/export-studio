import { HOT_CUE_KINDS } from '../format'
import type { ActionHandler } from '../keybinds.svelte'
import { HOT_CUE_ACTIONS, type ActionId } from '../keymap'
import type { Player } from '../player.svelte'

/** The playback and cue shortcuts, acting on `player`. Register them for the focused deck. */
export function deckKeys(player: Player): Partial<Record<ActionId, ActionHandler>> {
  return {
    'deck.playPause': () => player.toggle(),
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
    ...Object.fromEntries(
      HOT_CUE_ACTIONS.map((id, i) => [
        id,
        { down: () => player.hotCueDown(HOT_CUE_KINDS[i]), up: () => player.hotCueUp(HOT_CUE_KINDS[i]) }
      ])
    ),
    'cue.previousMemory': () => player.callMemoryCue(-1),
    'cue.nextMemory': () => player.callMemoryCue(1),
    'cue.addMemory': () => void player.addMemoryCue(),
    'cue.deleteAtPlayhead': () => void player.deleteCueAtPlayhead()
  }
}
