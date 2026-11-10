import { on } from 'svelte/events'
import { createSubscriber } from 'svelte/reactivity'

let held = false

const subscribe = createSubscriber((update) => {
  const follow = (e: KeyboardEvent): void => {
    if (held === e.shiftKey) return
    held = e.shiftKey
    update()
  }
  const release = (): void => {
    held = false
    update()
  }
  const listeners = [on(window, 'keydown', follow), on(window, 'keyup', follow), on(window, 'blur', release)]
  return () => listeners.forEach((stop) => stop())
})

export function shiftHeld(): boolean {
  subscribe()
  return held
}
