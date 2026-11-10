import {
  ACTION_IDS,
  ACTIONS,
  type ActionId,
  type ActionSpec,
  chordFromEvent,
  chordTypesText,
  DEFAULT_BINDINGS,
  DEFAULT_LAYER_MODIFIER,
  isLayerModifier,
  type LayerModifier,
  withoutModifier
} from './keymap'
import { getSetting, setSetting } from './settings'

export type ActionHandler = (() => void) | { down: () => void; up: () => void }

/**
 * `plain` handlers answer an action's own chords. `modified` handlers answer the same chords with
 * the layer modifier added, ahead of whatever action that chord is bound to itself: mix mode puts
 * deck B there, so every deck shortcut works on either deck.
 */
export type KeyLayer = 'plain' | 'modified'

type Overrides = Partial<Record<ActionId, string[]>>

const STORAGE_KEY = 'keybinds'
const LAYER_MODIFIER_KEY = 'layerModifier'
const TEXT_INPUTS = new Set(['text', 'search', 'email', 'password', 'url', 'number'])

function storedOverrides(): Overrides {
  try {
    const parsed = JSON.parse(getSetting(STORAGE_KEY) ?? '{}')
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

function storedLayerModifier(): LayerModifier {
  const stored = getSetting(LAYER_MODIFIER_KEY)
  return isLayerModifier(stored) ? stored : DEFAULT_LAYER_MODIFIER
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
  layerModifier = $state<LayerModifier>(storedLayerModifier())
  readonly bindings: Record<ActionId, readonly string[]> = $derived({ ...DEFAULT_BINDINGS, ...this.overrides })
  private readonly actionByChord = $derived.by(() => {
    const map = new Map<string, ActionId>()
    for (const id of ACTION_IDS) for (const chord of this.bindings[id]) if (!map.has(chord)) map.set(chord, id)
    return map
  })
  private handlers = new Map<ActionId, ActionHandler>()
  private modified = new Map<ActionId, ActionHandler>()
  /** The handler each held key went down on, so its release reaches the same one. */
  private held = new Map<string, ActionHandler>()
  private swallowed = new Set<string>()
  private suspended = 0

  register(handlers: Partial<Record<ActionId, ActionHandler>>, layer: KeyLayer = 'plain'): () => void {
    const map = layer === 'modified' ? this.modified : this.handlers
    const entries = Object.entries(handlers) as [ActionId, ActionHandler][]
    for (const [id, handler] of entries) map.set(id, handler)
    return () => {
      for (const [id, handler] of entries) if (map.get(id) === handler) map.delete(id)
    }
  }

  /** The action and handler a chord reaches: the modified layer first, then the chord's own action. */
  private resolve(chord: string): [ActionId, ActionHandler] | null {
    const unmodified = this.modified.size ? withoutModifier(chord, this.layerModifier) : null
    if (unmodified) {
      const id = this.actionFor(unmodified)
      const handler = id && this.modified.get(id)
      if (id && handler) return [id, handler]
    }
    const id = this.actionFor(chord)
    const handler = id && this.handlers.get(id)
    return id && handler ? [id, handler] : null
  }

  actionFor(chord: string): ActionId | undefined {
    return this.actionByChord.get(chord)
  }

  isCustomised(id: ActionId): boolean {
    return !sameChords(this.bindings[id], DEFAULT_BINDINGS[id])
  }

  get customised(): boolean {
    return this.layerModifier !== DEFAULT_LAYER_MODIFIER || ACTION_IDS.some((id) => this.isCustomised(id))
  }

  setLayerModifier(modifier: LayerModifier): void {
    this.releaseAll()
    this.layerModifier = modifier
    setSetting(LAYER_MODIFIER_KEY, modifier)
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
    this.setLayerModifier(DEFAULT_LAYER_MODIFIER)
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
    setSetting(STORAGE_KEY, JSON.stringify($state.snapshot(this.overrides)))
  }

  keydown = (e: KeyboardEvent): void => {
    if (this.suspended || e.defaultPrevented || e.isComposing) return
    const chord = chordFromEvent(e)
    const match = chord ? this.resolve(chord) : null
    if (!chord || !match) return
    const [id, handler] = match
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
    this.held.set(e.code, handler)
    handler.down()
  }

  keyup = (e: KeyboardEvent): void => {
    if (this.swallowed.delete(e.code)) e.preventDefault()
    const handler = this.held.get(e.code)
    if (!handler) return
    this.held.delete(e.code)
    release(handler)
  }

  releaseAll = (): void => {
    const handlers = [...this.held.values()]
    this.held.clear()
    this.swallowed.clear()
    for (const handler of handlers) release(handler)
  }
}

function release(handler: ActionHandler): void {
  if (typeof handler !== 'function') handler.up()
}
