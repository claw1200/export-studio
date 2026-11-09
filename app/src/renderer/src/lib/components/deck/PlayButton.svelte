<script lang="ts">
  import { Pause, Play, Square } from '@lucide/svelte'
  import DeckButton from './DeckButton.svelte'

  let {
    playing,
    disabled = false,
    onclick,
    label,
    title,
    stopsInsteadOfPausing = false,
    size = 'sm'
  }: {
    playing: boolean
    disabled?: boolean
    onclick: () => void
    label: string
    title?: string
    stopsInsteadOfPausing?: boolean
    size?: 'sm' | 'lg'
  } = $props()

  const Icon = $derived(playing ? (stopsInsteadOfPausing ? Square : Pause) : Play)
  const icon = $derived(size === 'lg' ? 'size-5' : 'size-3')
</script>

<DeckButton tone="play" lit={playing} {size} {disabled} {onclick} aria-label={label} {title}>
  <Icon class={[icon, 'fill-current drop-shadow-sm']} strokeWidth={0} />
</DeckButton>
