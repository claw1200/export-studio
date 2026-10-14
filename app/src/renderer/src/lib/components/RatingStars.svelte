<script lang="ts">
  import { Star } from '@lucide/svelte'

  let {
    value,
    interactive = false,
    starClass,
    hitClass = '',
    label = '',
    onchange,
    class: klass = ''
  }: {
    value: number | null
    interactive?: boolean
    starClass: string
    hitClass?: string
    label?: string
    onchange?: (rating: number) => void
    class?: string
  } = $props()

  function choose(n: number): void {
    onchange?.(value === n ? 0 : n)
  }
</script>

<span
  class={['flex', klass]}
  role={interactive ? 'radiogroup' : label ? 'img' : undefined}
  aria-label={label || undefined}
>
  {#each [1, 2, 3, 4, 5] as n (n)}
    {@const lit = value != null && n <= value}
    {#if interactive}
      <button
        type="button"
        class={['grid place-items-center rounded hover:bg-hover', hitClass]}
        role="radio"
        aria-checked={value === n}
        aria-label="{n} star{n === 1 ? '' : 's'}"
        onclick={() => choose(n)}
      >
        <Star class={[starClass, lit ? 'fill-warn text-warn' : 'text-line']} strokeWidth={lit ? 0 : 2} />
      </button>
    {:else}
      <Star class={[starClass, lit ? 'fill-warn text-warn' : 'text-line']} strokeWidth={lit ? 0 : 2} />
    {/if}
  {/each}
</span>
