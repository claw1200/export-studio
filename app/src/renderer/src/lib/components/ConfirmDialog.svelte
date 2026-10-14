<script lang="ts">
  import type { Snippet } from 'svelte'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'

  let {
    title,
    message,
    confirmLabel,
    danger = false,
    onconfirm,
    onclose,
    children
  }: {
    title: string
    message: string
    confirmLabel: string
    danger?: boolean
    onconfirm: () => void
    onclose: () => void
    children?: Snippet
  } = $props()

  const { keybinds } = getApp()
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto w-[380px] max-w-[calc(100vw-48px)] rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label={title}
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <form
    method="dialog"
    class="p-5"
    onsubmit={(e) => {
      if (e.submitter?.getAttribute('value') === 'confirm') onconfirm()
    }}
  >
    <h2 class="text-[13.5px] font-semibold">{title}</h2>
    <p class="mt-1.5 text-[12.5px] leading-relaxed text-muted">{message}</p>
    {@render children?.()}
    <div class="mt-5 flex justify-end gap-2">
      <button value="cancel" class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg">
        Cancel
      </button>
      <button
        value="confirm"
        class={[
          'h-7 rounded-md px-3 text-[12.5px] font-medium',
          danger ? 'bg-danger/90 text-bg hover:bg-danger' : 'bg-accent text-bg hover:bg-accent/90'
        ]}
      >
        {confirmLabel}
      </button>
    </div>
  </form>
</dialog>
