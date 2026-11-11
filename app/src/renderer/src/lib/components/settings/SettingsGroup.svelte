<script lang="ts">
  import type { Snippet } from 'svelte'

  let { title, children }: { title: string; children: Snippet } = $props()
</script>

<section class="mt-5 first:mt-0">
  <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">{title}</h3>
  {@render children()}
</section>
