import type { Component } from 'svelte'
import type { Player } from '../player.svelte'
import type { ConvertPreset, Track } from '../types'

/** What the shell does with tracks, whichever mode shows them. Each opens one of the shell's dialogs. */
export interface TrackActions {
  edit: (tracks: Track[]) => void
  remove: (tracks: Track[]) => void
  analyse: (tracks: Track[]) => void
  convert: (tracks: Track[], preset: ConvertPreset) => void
  autoRelocate: (tracks: Track[]) => void
}

/** What the shell hands the mode it shows. */
export interface ModeProps {
  /** The mode's players, in the order its definition's `decks` names them. */
  players: Player[]
  actions: TrackActions
  inspectorOpen: boolean
}

/**
 * A layout of the main window below the title bar. The shell keeps the title bar, status bar,
 * dialogs and library-wide shortcuts; the mode lays out its decks and library browser, decides
 * what selecting and opening a row does, and registers any shortcuts of its own.
 */
export interface ModeDefinition {
  id: string
  label: string
  /**
   * Keys of the players this mode shows, claimed from `Decks` when it is entered. Players live for
   * the session, so a mode finds its decks as it left them; two modes naming the same key share one.
   */
  decks: readonly string[]
  /** Shows the info panel beside the library. Without it the title bar's toggle is hidden too. */
  inspector: boolean
  component: Component<ModeProps>
}
