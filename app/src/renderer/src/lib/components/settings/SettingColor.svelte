<script lang="ts">
  import { RotateCcw } from '@lucide/svelte'
  import SettingText from './SettingText.svelte'

  let {
    label,
    description,
    color,
    defaultColor,
    onchange
  }: {
    label: string
    description: string
    color: string
    defaultColor: string
    onchange: (color: string) => void
  } = $props()
</script>

<div class="flex items-center gap-3 rounded-md px-2 py-2">
  <SettingText {label} {description} />
  <input
    class="h-7 w-10 shrink-0 cursor-pointer overflow-hidden rounded-md border border-line bg-transparent p-0 [&::-webkit-color-swatch]:border-none [&::-webkit-color-swatch-wrapper]:p-0"
    type="color"
    aria-label={label}
    value={color}
    oninput={(e) => onchange(e.currentTarget.value)}
    onchange={(e) => onchange(e.currentTarget.value)}
  />
  <span class="w-16 shrink-0 text-right font-mono text-[11px] text-muted uppercase tabular-nums">{color}</span>
  <button
    class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:invisible"
    disabled={color === defaultColor}
    onclick={() => onchange(defaultColor)}
    aria-label="Reset {label.toLowerCase()}"
    title="Reset to default"
  >
    <RotateCcw class="size-3" />
  </button>
</div>
