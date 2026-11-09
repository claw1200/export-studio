import { HOT_CUE_KINDS } from '../format'
import type { ActionHandler } from '../keybinds.svelte'
import { HOT_CUE_ACTIONS, type ActionId } from '../keymap'
import type { Player } from '../player.svelte'
import { BEAT_LOOP_BEATS } from './transport'

type DeckHandlers = Partial<Record<ActionId, ActionHandler>>

export function playOnlyKeys(player: Player, playPause: () => void = () => player.toggle()): DeckHandlers {
  return {
    'deck.playPause': playPause,
    'deck.cue': { down: () => player.cueDown(), up: () => player.cueUp() },
    'deck.loopIn': () => player.setLoopIn(),
    'deck.loopOut': () => player.setLoopOut(),
    'deck.reloopExit': () => player.reloopExit(),
    'deck.beatLoop': () => player.beatLoop(BEAT_LOOP_BEATS),
    'deck.loopHalve': () => player.resizeLoop(0.5),
    'deck.loopDouble': () => player.resizeLoop(2),
    'deck.quantise': () => player.setQuantise(!player.quantise),
    'deck.seekBack': () => player.seek(player.position - 2),
    'deck.seekForward': () => player.seek(player.position + 2),
    'deck.seekBackFar': () => player.seek(player.position - 10),
    'deck.seekForwardFar': () => player.seek(player.position + 10),
    ...hotCueKeys(player, (kind) => player.hotCueDown(kind)),
    'cue.previousMemory': () => player.callMemoryCue(-1),
    'cue.nextMemory': () => player.callMemoryCue(1)
  }
}

export function deckKeys(player: Player): DeckHandlers {
  return {
    ...playOnlyKeys(player),
    ...hotCueKeys(player, (kind) => player.hotCueDownOrStore(kind)),
    'cue.addMemory': () => void player.addMemoryCue(),
    'cue.deleteAtPlayhead': () => void player.deleteCueAtPlayhead()
  }
}

function hotCueKeys(player: Player, down: (kind: number) => void): DeckHandlers {
  return Object.fromEntries(
    HOT_CUE_ACTIONS.map((id, i) => {
      const kind = HOT_CUE_KINDS[i]
      return [id, { down: () => down(kind), up: () => player.hotCueUp(kind) }]
    })
  )
}
