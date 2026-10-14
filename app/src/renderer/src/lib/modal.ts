import type { Attachment } from 'svelte/attachments'

export function suspendModal(keybinds: { suspend(): () => void }): Attachment<HTMLDialogElement> {
  return (dialog) => {
    const resume = keybinds.suspend()
    dialog.showModal()
    return resume
  }
}
