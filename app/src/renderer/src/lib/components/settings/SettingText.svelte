<script lang="ts">
  let { label, description }: { label: string; description: string } = $props()
</script>

<span class="min-w-0 flex-1">
  <span class="block text-[12.5px] font-medium">{label}</span>
  <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
</span>
