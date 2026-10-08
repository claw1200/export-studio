import { HOT_CUE_LETTERS } from './format'

export type ActionGroup = 'Playback' | 'Cues' | 'Library' | 'App'

export interface ActionSpec {
  label: string
  group: ActionGroup
  hold?: boolean
  repeat?: boolean
  /**
   * Where the shortcut still fires when focus is in a form field.
   * `never`: only outside fields. `controls`: also in sliders and other non-text controls.
   * `always`: also in text fields, but only for chords that cannot type a character.
   */
  fields?: 'never' | 'controls' | 'always'
}

export const HOT_CUE_ACTIONS = [
  'cue.hot1',
  'cue.hot2',
  'cue.hot3',
  'cue.hot4',
  'cue.hot5',
  'cue.hot6',
  'cue.hot7',
  'cue.hot8'
] as const

type HotCueAction = (typeof HOT_CUE_ACTIONS)[number]

const hotCueSpecs = Object.fromEntries(
  HOT_CUE_ACTIONS.map((id, i) => [
    id,
    { label: `Hot cue ${HOT_CUE_LETTERS[i]}`, group: 'Cues', hold: true, fields: 'controls' } satisfies ActionSpec
  ])
) as Record<HotCueAction, ActionSpec>

export const ACTIONS = {
  'deck.playPause': { label: 'Play / pause', group: 'Playback', fields: 'controls' },
  'deck.cue': { label: 'Cue', group: 'Playback', hold: true, fields: 'controls' },
  'deck.loopIn': { label: 'Loop in', group: 'Playback', fields: 'controls' },
  'deck.loopOut': { label: 'Loop out', group: 'Playback', fields: 'controls' },
  'deck.reloopExit': { label: 'Reloop / exit', group: 'Playback', fields: 'controls' },
  'deck.beatLoop': { label: '4-beat loop', group: 'Playback', fields: 'controls' },
  'deck.loopHalve': { label: 'Halve loop', group: 'Playback', fields: 'controls' },
  'deck.loopDouble': { label: 'Double loop', group: 'Playback', fields: 'controls' },
  'deck.quantise': { label: 'Toggle quantise', group: 'Playback', fields: 'controls' },
  'deck.gridEdit': { label: 'Toggle beatgrid editing', group: 'Playback', fields: 'controls' },
  'deck.seekBack': { label: 'Seek back 2s', group: 'Playback', repeat: true },
  'deck.seekForward': { label: 'Seek forward 2s', group: 'Playback', repeat: true },
  'deck.seekBackFar': { label: 'Seek back 10s', group: 'Playback', repeat: true },
  'deck.seekForwardFar': { label: 'Seek forward 10s', group: 'Playback', repeat: true },
  ...hotCueSpecs,
  'cue.previousMemory': { label: 'Previous memory cue', group: 'Cues', repeat: true },
  'cue.nextMemory': { label: 'Next memory cue', group: 'Cues', repeat: true },
  'cue.addMemory': { label: 'Add memory cue', group: 'Cues', fields: 'controls' },
  'cue.deleteAtPlayhead': { label: 'Delete cue at playhead', group: 'Cues', fields: 'controls' },
  'library.next': { label: 'Next track', group: 'Library', repeat: true, fields: 'always' },
  'library.previous': { label: 'Previous track', group: 'Library', repeat: true, fields: 'always' },
  'library.nextPage': { label: 'Down 10 tracks', group: 'Library', repeat: true, fields: 'always' },
  'library.previousPage': { label: 'Up 10 tracks', group: 'Library', repeat: true, fields: 'always' },
  'library.loadSelected': { label: 'Load and play selected track', group: 'Library', fields: 'always' },
  'library.search': { label: 'Search', group: 'Library', fields: 'always' },
  'library.reload': { label: 'Reload library', group: 'Library', fields: 'always' },
  'library.selectAll': { label: 'Select all tracks', group: 'Library' },
  'library.editInfo': { label: 'Edit track info', group: 'Library' },
  'library.removeFromPlaylist': { label: 'Remove from playlist', group: 'Library' },
  'app.settings': { label: 'Settings', group: 'App', fields: 'always' }
} satisfies Record<string, ActionSpec>

export type ActionId = keyof typeof ACTIONS

export const ACTION_IDS = Object.keys(ACTIONS) as ActionId[]

export const DEFAULT_BINDINGS: Record<ActionId, readonly string[]> = {
  'deck.playPause': ['Space'],
  'deck.cue': ['KeyC'],
  'deck.loopIn': ['KeyI'],
  'deck.loopOut': ['KeyO'],
  'deck.reloopExit': ['KeyP'],
  'deck.beatLoop': ['KeyL'],
  'deck.loopHalve': ['BracketLeft'],
  'deck.loopDouble': ['BracketRight'],
  'deck.quantise': ['KeyQ'],
  'deck.gridEdit': [],
  'deck.seekBack': [],
  'deck.seekForward': [],
  'deck.seekBackFar': ['Shift+ArrowLeft'],
  'deck.seekForwardFar': ['Shift+ArrowRight'],
  'cue.hot1': ['Digit1'],
  'cue.hot2': ['Digit2'],
  'cue.hot3': ['Digit3'],
  'cue.hot4': ['Digit4'],
  'cue.hot5': ['Digit5'],
  'cue.hot6': ['Digit6'],
  'cue.hot7': ['Digit7'],
  'cue.hot8': ['Digit8'],
  'cue.previousMemory': ['ArrowLeft'],
  'cue.nextMemory': ['ArrowRight'],
  'cue.addMemory': ['KeyM'],
  'cue.deleteAtPlayhead': ['Backspace'],
  'library.next': ['ArrowDown'],
  'library.previous': ['ArrowUp'],
  'library.nextPage': ['Shift+ArrowDown'],
  'library.previousPage': ['Shift+ArrowUp'],
  'library.loadSelected': ['Enter'],
  'library.search': ['Mod+KeyF'],
  'library.reload': ['Mod+KeyR'],
  'library.selectAll': ['Mod+KeyA'],
  'library.editInfo': ['Mod+KeyI'],
  'library.removeFromPlaylist': ['Mod+Backspace', 'Delete'],
  'app.settings': ['Mod+Comma']
}

const isMac = window.api.platform === 'darwin'

const MODIFIER_CODES = new Set([
  'ShiftLeft',
  'ShiftRight',
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'MetaLeft',
  'MetaRight',
  'CapsLock',
  'Fn'
])

export function chordFromEvent(e: KeyboardEvent): string | null {
  if (!e.code || MODIFIER_CODES.has(e.code)) return null
  const parts: string[] = []
  if (isMac ? e.metaKey : e.ctrlKey) parts.push('Mod')
  if (isMac ? e.ctrlKey : e.metaKey) parts.push(isMac ? 'Ctrl' : 'Meta')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  parts.push(e.code)
  return parts.join('+')
}

function splitChord(chord: string): { modifiers: string[]; code: string } {
  const parts = chord.split('+')
  return { modifiers: parts.slice(0, -1), code: parts[parts.length - 1] }
}

const NON_TEXT_KEYS = /^(Arrow|F\d|Page|Home$|End$|Enter$|NumpadEnter$|Escape$|Tab$|Insert$)/

export function chordTypesText(chord: string): boolean {
  const { modifiers, code } = splitChord(chord)
  if (modifiers.some((m) => m !== 'Shift')) return false
  return !NON_TEXT_KEYS.test(code)
}

const KEY_LABELS: Record<string, string> = {
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Space: 'Space',
  Backspace: isMac ? '⌫' : 'Backspace',
  Delete: isMac ? '⌦' : 'Del',
  Enter: isMac ? '↩' : 'Enter',
  NumpadEnter: isMac ? '⌤' : 'Enter',
  Escape: 'Esc',
  Tab: isMac ? '⇥' : 'Tab',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`'
}

const MODIFIER_LABELS: Record<string, string> = isMac
  ? { Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Mod: '⌘' }
  : { Mod: 'Ctrl', Meta: 'Win', Alt: 'Alt', Shift: 'Shift' }

const MODIFIER_ORDER = isMac ? ['Ctrl', 'Alt', 'Shift', 'Mod'] : ['Mod', 'Meta', 'Alt', 'Shift']

function keyLabel(code: string): string {
  if (KEY_LABELS[code]) return KEY_LABELS[code]
  const key = /^Key([A-Z])$/.exec(code) ?? /^Digit(\d)$/.exec(code)
  if (key) return key[1]
  const numpad = /^Numpad(\w+)$/.exec(code)
  if (numpad) return `Num ${numpad[1]}`
  return code
}

export function chordLabel(chord: string): string {
  const { modifiers, code } = splitChord(chord)
  const mods = MODIFIER_ORDER.filter((m) => modifiers.includes(m)).map((m) => MODIFIER_LABELS[m] ?? m)
  const key = keyLabel(code)
  return isMac ? mods.join('') + key : [...mods, key].join('+')
}
