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

<label class="flex items-start gap-3 rounded-md px-2 py-2">
  <SettingText {label} {description} />
  <select
    class="h-7 max-w-44 shrink-0 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
    aria-label={label}
    value={current}
    onchange={(e) => onpick(e.currentTarget.value as Id)}
  >
    {#each options as o (o.id)}
      <option value={o.id}>{o.label}</option>
    {/each}
  </select>
</label>
