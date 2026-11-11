<script lang="ts" generics="Id extends string">
  import type { Choice } from './choice'
  import SettingText from './SettingText.svelte'

  let {
    label,
    description,
    options,
    current,
    onpick
  }: { label: string; description: string; options: Choice<Id>[]; current: Id; onpick: (id: Id) => void } = $props()
</script>

<div class="flex items-start gap-3 rounded-md px-2 py-2">
  <SettingText {label} {description} />
  <div
    class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[11px] font-semibold tabular-nums"
    role="radiogroup"
    aria-label={label}
  >
    {#each options as o (o.id)}
      <button
        class={['rounded px-2 py-1', current === o.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted']}
        role="radio"
        aria-checked={current === o.id}
        onclick={() => onpick(o.id)}
      >
        {o.label}
      </button>
    {/each}
  </div>
</div>
