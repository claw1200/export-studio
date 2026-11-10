import { on } from 'svelte/events'
import { createSubscriber } from 'svelte/reactivity'
import { type LayerModifier, layerModifierPressed } from './keymap'

let lastEvent: KeyboardEvent | null = null

const subscribe = createSubscriber((update) => {
  const follow = (e: KeyboardEvent): void => {
    lastEvent = e
    update()
  }
  const release = (): void => {
    lastEvent = null
    update()
  }
  const listeners = [on(window, 'keydown', follow), on(window, 'keyup', follow), on(window, 'blur', release)]
  return () => listeners.forEach((stop) => stop())
})

export function layerModifierHeld(modifier: LayerModifier): boolean {
  subscribe()
  return lastEvent != null && layerModifierPressed(lastEvent, modifier)
}
