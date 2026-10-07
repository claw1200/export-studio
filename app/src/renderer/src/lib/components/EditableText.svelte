<script lang="ts">
  let {
    value,
    label,
    editable,
    onsave,
    placeholder = '',
    multiline = false,
    numeric = false,
    class: className = ''
  }: {
    value: string
    label: string
    editable: boolean
    onsave: (value: string) => Promise<unknown>
    placeholder?: string
    multiline?: boolean
    numeric?: boolean
    class?: string
  } = $props()

  let editing = $state(false)
  let pending = $state<string | null>(null)
  const shown = $derived(pending ?? value)

  function focus(node: HTMLInputElement | HTMLTextAreaElement): void {
    node.focus()
    node.select()
  }

  async function finish(node: HTMLInputElement | HTMLTextAreaElement, save: boolean): Promise<void> {
    if (!editing) return
    editing = false
    const next = node.value.trim()
    if (!save || next === value.trim()) return
    pending = next
    try {
      await onsave(next)
    } finally {
      pending = null
    }
  }

  function onkeydown(e: KeyboardEvent & { currentTarget: HTMLInputElement | HTMLTextAreaElement }): void {
    e.stopPropagation()
    if (e.key === 'Escape') {
      e.preventDefault()
      void finish(e.currentTarget, false)
    } else if (e.key === 'Enter' && !(multiline && e.shiftKey)) {
      e.preventDefault()
      void finish(e.currentTarget, true)
    }
  }

  const fieldClass =
    'w-full min-w-0 rounded border border-accent/60 bg-bg px-1.5 py-0.5 text-fg select-text focus:outline-none'
</script>

{#if editing}
  {#if multiline}
    <textarea
      {@attach focus}
      {value}
      rows="3"
      spellcheck="false"
      aria-label={label}
      class={[fieldClass, 'resize-none leading-snug', className]}
      {onkeydown}
      onblur={(e) => void finish(e.currentTarget, true)}
    ></textarea>
  {:else}
    <input
      {@attach focus}
      {value}
      spellcheck="false"
      inputmode={numeric ? 'numeric' : undefined}
      aria-label={label}
      class={[fieldClass, className]}
      {onkeydown}
      onblur={(e) => void finish(e.currentTarget, true)}
    />
  {/if}
{:else if editable}
  <button
    type="button"
    class={[
      '-mx-1 block max-w-[calc(100%+0.5rem)] min-w-0 cursor-text rounded px-1 hover:bg-hover',
      multiline ? 'break-words whitespace-pre-wrap' : 'truncate',
      !shown && 'text-faint/70 italic',
      className
    ]}
    title="Edit {label.toLowerCase()}"
    onclick={() => (editing = true)}
  >
    {shown || placeholder}
  </button>
{:else}
  <span class={['select-text', className]}>{shown}</span>
{/if}
