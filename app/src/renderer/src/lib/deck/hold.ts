import type { Attachment } from 'svelte/attachments'

/** A press-and-hold button: `release` follows `press` however the pointer lets go. */
export function hold(press: () => void, release: () => void): Attachment<HTMLElement> {
  return (node) => {
    const down = (e: PointerEvent): void => {
      if (e.button !== 0) return
      node.setPointerCapture(e.pointerId)
      press()
    }
    const ends = ['pointerup', 'pointercancel', 'lostpointercapture'] as const
    node.addEventListener('pointerdown', down)
    for (const end of ends) node.addEventListener(end, release)
    return () => {
      node.removeEventListener('pointerdown', down)
      for (const end of ends) node.removeEventListener(end, release)
    }
  }
}
