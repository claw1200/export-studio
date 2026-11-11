<script lang="ts">
  import SettingText from './SettingText.svelte'

  let {
    label,
    description,
    min,
    max,
    step,
    value,
    valueText,
    onchange
  }: {
    label: string
    description: string
    min: number
    max: number
    step: number
    value: number
    valueText: string
    onchange: (value: number) => void
  } = $props()
</script>

<div class="flex items-center gap-3 rounded-md px-2 py-2">
  <SettingText {label} {description} />
  <input
    class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
    type="range"
    {min}
    {max}
    {step}
    aria-label={label}
    aria-valuetext={valueText}
    {value}
    oninput={(e) => onchange(Number(e.currentTarget.value))}
  />
  <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">{valueText}</span>
</div>
