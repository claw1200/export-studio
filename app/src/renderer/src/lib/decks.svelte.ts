import { ENGINE_DECKS } from '../../../shared/engine'
import { Audition } from './audition.svelte'
import { Output } from './deck/output.svelte'
import { DeckPrefs } from './deck/prefs.svelte'
import { MixPrefs } from './mix/prefs.svelte'
import { Player } from './player.svelte'

/** The browser preview keeps the last engine deck, so adding decks never moves it. */
const AUDITION_SLOT = ENGINE_DECKS - 1

/**
 * Every deck the app plays through, and what they share: the master output, deck preferences and
 * the browser preview. Players are made on first use under a key (a mode names the ones it shows)
 * and live for the whole session, so switching modes keeps what each deck had loaded.
 *
 * `focused` is the deck that app-wide controls act on: transport keys, the inspector's cue jumps,
 * the status bar. A mode with one deck focuses it; a mode with several moves the focus.
 */
export class Decks {
  readonly prefs = new DeckPrefs()
  /** How mix mode's crossfader, bass swap and key highlighting behave. */
  readonly mixPrefs = new MixPrefs()
  readonly output = new Output()
  readonly audition = new Audition(AUDITION_SLOT)
  focused = $state.raw<Player | null>(null)
  /** Mix mode's master deck while mix mode is shown; the browser highlights keys that mix with it. */
  mixMaster = $state.raw<Player | null>(null)

  private players = $state.raw<Player[]>([])
  private byKey = new Map<string, Player>()
  private fileListeners = new Set<(id: string, missing: boolean) => void>()

  constructor() {
    this.audition.onBegan = () => {
      for (const player of this.players) player.yieldToPreview()
    }
    this.audition.onEnded = () => Player.resumeAfterPreview(this.players)
  }

  /** Every player made so far, in the order they were. */
  get all(): readonly Player[] {
    return this.players
  }

  /** The player kept under `key`, made on the next free engine deck the first time it is asked for. */
  claim(key: string): Player {
    const existing = this.byKey.get(key)
    if (existing) return existing
    const slot = this.players.length
    if (slot >= AUDITION_SLOT) throw new Error(`No engine deck is free for “${key}”; raise ENGINE_DECKS`)
    const player = new Player(slot, this.prefs)
    player.onDeckPlay = () => this.audition.stop()
    player.onFileChecked((id, missing) => {
      for (const listener of this.fileListeners) listener(id, missing)
    })
    this.byKey.set(key, player)
    this.players = [...this.players, player]
    return player
  }

  /**
   * Shows the players under `keys`: claims them, pauses every other deck so a hidden one is never
   * heard, and focuses the first unless the focus is already one of them.
   */
  enter(keys: readonly string[]): Player[] {
    const shown = keys.map((key) => this.claim(key))
    for (const player of this.players) if (!shown.includes(player) && player.playing) player.pause()
    if (!this.focused || !shown.includes(this.focused)) this.focused = shown[0] ?? null
    return shown
  }

  focus(player: Player): void {
    if (this.players.includes(player)) this.focused = player
  }

  /** Players with `trackId` loaded. */
  withTrack(trackId: string): Player[] {
    return this.players.filter((player) => player.track?.id === trackId)
  }

  /** Ejects every deck whose track matches. */
  eject(matches: (trackId: string) => boolean = () => true): void {
    for (const player of this.players) {
      const id = player.track?.id
      if (id && matches(id)) player.eject()
    }
  }

  /** Reads every loaded track's detail again, e.g. after editing was unlocked. */
  async refreshDetail(): Promise<void> {
    await Promise.all(this.players.map((player) => player.refreshDetail()))
  }

  /** Called when any deck learns whether its track's file is there. */
  onFileChecked(listener: (id: string, missing: boolean) => void): () => void {
    this.fileListeners.add(listener)
    return () => this.fileListeners.delete(listener)
  }
}
