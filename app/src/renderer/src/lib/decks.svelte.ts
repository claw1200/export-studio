import { ENGINE_DECKS } from '../../../shared/engine'
import { Audition } from './audition.svelte'
import { Output } from './deck/output.svelte'
import { DeckPrefs } from './deck/prefs.svelte'
import { Listeners } from './listeners'
import { MixPrefs } from './mix/prefs.svelte'
import { Player } from './player.svelte'

const AUDITION_ENGINE_DECK = ENGINE_DECKS - 1

/** Players live for the whole session, so switching modes keeps what each deck had loaded. */
export class Decks {
  readonly prefs = new DeckPrefs()
  readonly mixPrefs = new MixPrefs()
  readonly output = new Output()
  readonly audition = new Audition(AUDITION_ENGINE_DECK)
  focused = $state.raw<Player | null>(null)
  mixMaster = $state.raw<Player | null>(null)

  private players = $state.raw<Player[]>([])
  private byKey = new Map<string, Player>()
  private fileChecked = new Listeners<[id: string, missing: boolean]>()

  constructor() {
    this.audition.onBegan = () => {
      for (const player of this.players) player.yieldToPreview()
    }
    this.audition.onEnded = () => Player.resumeAfterPreview(this.players)
  }

  get all(): readonly Player[] {
    return this.players
  }

  claim(key: string): Player {
    const existing = this.byKey.get(key)
    if (existing) return existing
    const engineDeck = this.players.length
    if (engineDeck >= AUDITION_ENGINE_DECK) throw new Error(`No engine deck is free for “${key}”; raise ENGINE_DECKS`)
    const player = new Player(engineDeck, this.prefs)
    player.onTakeSpeakers = () => this.audition.stop()
    player.onFileChecked((id, missing) => this.fileChecked.notify(id, missing))
    this.byKey.set(key, player)
    this.players = [...this.players, player]
    return player
  }

  enter(keys: readonly string[]): Player[] {
    const shown = keys.map((key) => this.claim(key))
    const hidden = this.players.filter((player) => !shown.includes(player))
    for (const player of hidden) if (player.playing) player.pause()
    const focusIsShown = this.focused != null && shown.includes(this.focused)
    if (!focusIsShown) this.focused = shown[0] ?? null
    return shown
  }

  focus(player: Player): void {
    if (this.players.includes(player)) this.focused = player
  }

  withTrack(trackId: string): Player[] {
    return this.players.filter((player) => player.track?.id === trackId)
  }

  ejectAll(): void {
    this.ejectWhere(() => true)
  }

  ejectWhere(matches: (trackId: string) => boolean): void {
    for (const player of this.players) {
      const id = player.track?.id
      if (id && matches(id)) player.eject()
    }
  }

  async refreshDetail(): Promise<void> {
    await Promise.all(this.players.map((player) => player.refreshDetail()))
  }

  onFileChecked(listener: (id: string, missing: boolean) => void): () => void {
    return this.fileChecked.add(listener)
  }
}
