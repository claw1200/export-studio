import {
  ACTION_IDS,
  ACTIONS,
  type ActionId,
  type ActionSpec,
  chordFromEvent,
  chordTypesText,
  DEFAULT_BINDINGS
} from './keymap'

export type ActionHandler = (() => void) | { down: () => void; up: () => void }

type Overrides = Partial<Record<ActionId, string[]>>

const STORAGE_KEY = 'keybinds'
const TEXT_INPUTS = new Set(['text', 'search', 'email', 'password', 'url', 'number'])

function storedOverrides(): Overrides {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}')
    const overrides: Overrides = {}
    for (const id of ACTION_IDS) {
      const chords = parsed?.[id]
      if (Array.isArray(chords)) overrides[id] = chords.filter((c): c is string => typeof c === 'string')
    }
    return overrides
  } catch {
    return {}
  }
}

function sameChords(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((c) => b.includes(c))
}

function fieldKind(target: EventTarget | null): 'text' | 'control' | null {
  if (!(target instanceof HTMLElement)) return null
  if (target.isContentEditable || target.tagName === 'TEXTAREA') return 'text'
  if (target instanceof HTMLInputElement) return TEXT_INPUTS.has(target.type) ? 'text' : 'control'
  if (target.tagName === 'SELECT') return 'control'
  return null
}

function firesIn(spec: ActionSpec, chord: string, target: EventTarget | null): boolean {
  const field = fieldKind(target)
  const policy = spec.fields ?? 'never'
  if (!field) return true
  if (field === 'text') return policy === 'always' && !chordTypesText(chord)
  return policy !== 'never'
}

export class Keybinds {
  overrides = $state<Overrides>(storedOverrides())
  readonly bindings: Record<ActionId, readonly string[]> = $derived({ ...DEFAULT_BINDINGS, ...this.overrides })
  private readonly actionByChord = $derived.by(() => {
    const map = new Map<string, ActionId>()
    for (const id of ACTION_IDS) for (const chord of this.bindings[id]) if (!map.has(chord)) map.set(chord, id)
    return map
  })
  private handlers = new Map<ActionId, ActionHandler>()
  private held = new Map<string, ActionId>()
  private swallowed = new Set<string>()
  private suspended = 0

  register(handlers: Partial<Record<ActionId, ActionHandler>>): () => void {
    const entries = Object.entries(handlers) as [ActionId, ActionHandler][]
    for (const [id, handler] of entries) this.handlers.set(id, handler)
    return () => {
      for (const [id, handler] of entries) if (this.handlers.get(id) === handler) this.handlers.delete(id)
    }
  }

  actionFor(chord: string): ActionId | undefined {
    return this.actionByChord.get(chord)
  }

  isCustomised(id: ActionId): boolean {
    return !sameChords(this.bindings[id], DEFAULT_BINDINGS[id])
  }

  get customised(): boolean {
    return ACTION_IDS.some((id) => this.isCustomised(id))
  }

  /** Returns the action the chord was taken from, if any. */
  addChord(id: ActionId, chord: string): ActionId | undefined {
    const owner = this.actionFor(chord)
    if (owner === id) return undefined
    this.setBinding(id, [...this.bindings[id], chord])
    return owner
  }

  removeChord(id: ActionId, chord: string): void {
    this.setBinding(
      id,
      this.bindings[id].filter((c) => c !== chord)
    )
  }

  setBinding(id: ActionId, chords: readonly string[]): void {
    const next = [...new Set(chords)]
    for (const other of ACTION_IDS) {
      if (other === id) continue
      const kept = this.bindings[other].filter((c) => !next.includes(c))
      if (kept.length !== this.bindings[other].length) this.override(other, kept)
    }
    this.override(id, next)
    this.save()
  }

  resetBinding(id: ActionId): void {
    this.setBinding(id, DEFAULT_BINDINGS[id])
  }

  resetAll(): void {
    this.overrides = {}
    this.save()
  }

  private override(id: ActionId, chords: string[]): void {
    if (sameChords(chords, DEFAULT_BINDINGS[id])) delete this.overrides[id]
    else this.overrides[id] = chords
  }

  suspend(): () => void {
    this.suspended++
    this.releaseAll()
    let resumed = false
    return () => {
      if (resumed) return
      resumed = true
      this.suspended--
    }
  }

  private save(): void {
    localStorage.setItem(STORAGE_KEY, JSON.stringify($state.snapshot(this.overrides)))
  }

  keydown = (e: KeyboardEvent): void => {
    if (this.suspended || e.defaultPrevented || e.isComposing) return
    const chord = chordFromEvent(e)
    const id = chord ? this.actionFor(chord) : undefined
    const handler = id && this.handlers.get(id)
    if (!chord || !id || !handler) return
    const spec: ActionSpec = ACTIONS[id]
    if (!firesIn(spec, chord, e.target)) return
    e.preventDefault()
    this.swallowed.add(e.code)
    if (e.repeat && !spec.repeat) return
    if (typeof handler === 'function') {
      handler()
      return
    }
    if (this.held.has(e.code)) return
    this.held.set(e.code, id)
    handler.down()
  }

  keyup = (e: KeyboardEvent): void => {
    if (this.swallowed.delete(e.code)) e.preventDefault()
    const id = this.held.get(e.code)
    if (!id) return
    this.held.delete(e.code)
    this.release(id)
  }

  releaseAll = (): void => {
    const ids = [...this.held.values()]
    this.held.clear()
    this.swallowed.clear()
    for (const id of ids) this.release(id)
  }

  private release(id: ActionId): void {
    const handler = this.handlers.get(id)
    if (handler && typeof handler !== 'function') handler.up()
  }
}
