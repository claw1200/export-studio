<script lang="ts">
  import { Pause, Play, Square } from '@lucide/svelte'
  import DeckButton from './DeckButton.svelte'

  /**
   * The round green play button, lit while playing. While playing it shows pause, or stop with
   * `stop` set, for a button that puts the playhead back instead of holding it.
   */
  let {
    playing,
    disabled = false,
    onclick,
    label,
    title,
    stop = false,
    size = 'sm'
  }: {
    playing: boolean
    disabled?: boolean
    onclick: () => void
    label: string
    title?: string
    stop?: boolean
    size?: 'sm' | 'lg'
  } = $props()

  const Icon = $derived(playing ? (stop ? Square : Pause) : Play)
  const icon = $derived(size === 'lg' ? 'size-5' : 'size-3')
</script>

<DeckButton tone="play" lit={playing} {size} {disabled} {onclick} aria-label={label} {title}>
  <Icon class={[icon, 'fill-current drop-shadow-sm']} strokeWidth={0} />
</DeckButton>
