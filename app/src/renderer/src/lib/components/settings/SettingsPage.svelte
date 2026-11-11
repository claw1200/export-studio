<script lang="ts">
  import type { Snippet } from 'svelte'
  import { RotateCcw, X } from '@lucide/svelte'
  import type { Resettable } from './resettable'

  let {
    title,
    defaults,
    headerTools,
    footerHint,
    children
  }: {
    title: string
    defaults?: Resettable
    headerTools?: Snippet
    footerHint?: Snippet
    children: Snippet
  } = $props()
</script>

<div class="flex min-w-0 flex-1 flex-col">
  <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
    <h2 class="mr-auto text-[13px] font-semibold">{title}</h2>
    {@render headerTools?.()}
    <button
      class="grid size-7 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg"
      onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      aria-label="Close settings"
    >
      <X class="size-4" />
    </button>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
    {@render children()}
  </div>

  {#if defaults}
    <footer class="flex h-11 shrink-0 items-center gap-3 border-t border-line-soft px-5 text-[11.5px]">
      {@render footerHint?.()}
      <button
        class="ml-auto flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
        disabled={defaults.isDefault()}
        onclick={defaults.reset}
      >
        <RotateCcw class="size-3" />
        Reset to defaults
      </button>
    </footer>
  {/if}
</div>
