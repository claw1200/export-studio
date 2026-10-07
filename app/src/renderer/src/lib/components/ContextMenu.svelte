<script lang="ts">
  import { getApp } from '$lib/context'
  import type { MenuRequest } from '$lib/menu'
  import MenuPanel from './MenuPanel.svelte'

  let { menu, onclose }: { menu: MenuRequest; onclose: () => void } = $props()

  const { keybinds } = getApp()
  const items = $derived(typeof menu.items === 'function' ? menu.items() : menu.items)

  function suspendKeys(): () => void {
    return keybinds.suspend()
  }
</script>

<div class="contents" {@attach suspendKeys}>
  <button
    class="fixed inset-0 z-40 cursor-default"
    aria-label="Close menu"
    tabindex="-1"
    onclick={onclose}
    oncontextmenu={(e) => {
      e.preventDefault()
      onclose()
    }}
  ></button>
  <MenuPanel {items} x={menu.x} y={menu.y} flipX={menu.x} label={menu.label} {onclose} />
</div>
