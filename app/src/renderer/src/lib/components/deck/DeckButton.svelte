<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { HTMLButtonAttributes } from 'svelte/elements'

  /**
   * A round player-style button: a brushed silver face inside a coloured ring that glows while
   * lit. Any other attributes, attachments included, go on the button itself.
   */
  let {
    tone,
    lit,
    size = 'sm',
    children,
    ...rest
  }: HTMLButtonAttributes & {
    tone: 'play' | 'cue'
    lit: boolean
    size?: 'sm' | 'lg'
    children: Snippet
  } = $props()

  // Written out in full so Tailwind can find each class.
  const ring = {
    play: { lit: 'bg-play shadow-[0_0_10px] shadow-play/60', unlit: 'bg-play/45' },
    cue: { lit: 'bg-cue shadow-[0_0_10px] shadow-cue/60', unlit: 'bg-cue/45' }
  }
</script>

<button
  {...rest}
  class={[
    'deck-button shrink-0 touch-none rounded-full transition select-none disabled:cursor-not-allowed disabled:opacity-30',
    size === 'lg' ? 'size-14 p-[3px]' : 'size-8 p-[2px]',
    ring[tone][lit ? 'lit' : 'unlit']
  ]}
>
  <span class="face grid size-full place-items-center rounded-full text-white">
    {@render children()}
  </span>
</button>

<style>
  .face {
    background:
      radial-gradient(circle, transparent 35%, rgb(0 0 0 / 0.35)),
      conic-gradient(
        #3c3d40,
        #8c8d91 10%,
        #45464a 22%,
        #9a9ba0 36%,
        #3c3d40 50%,
        #8c8d91 60%,
        #45464a 72%,
        #9a9ba0 86%,
        #3c3d40
      );
    box-shadow:
      inset 0 1px 1px rgb(255 255 255 / 0.3),
      inset 0 -2px 4px rgb(0 0 0 / 0.5);
    transition: filter 120ms;
  }
  .deck-button:not(:disabled):hover .face {
    filter: brightness(1.15);
  }
  .deck-button:not(:disabled):active .face {
    filter: brightness(0.85);
  }
</style>
