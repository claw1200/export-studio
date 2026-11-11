<script lang="ts">
  import SettingText from './SettingText.svelte'

  let { label, description, on, onflip }: { label: string; description: string; on: boolean; onflip: () => void } =
    $props()
</script>

<button
  class="flex w-full items-start gap-3 rounded-md px-2 py-2 text-left enabled:hover:bg-hover"
  role="switch"
  aria-checked={on}
  onclick={onflip}
>
  <SettingText {label} {description} />
  <span
    class={['relative mt-0.5 h-[18px] w-8 shrink-0 rounded-full transition-colors', on ? 'bg-accent' : 'bg-line']}
    aria-hidden="true"
  >
    <span
      class={[
        'absolute top-0.5 left-0.5 size-3.5 rounded-full bg-white shadow transition-transform',
        on && 'translate-x-3.5'
      ]}
    ></span>
  </span>
</button>
