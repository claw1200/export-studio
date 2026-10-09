<script lang="ts">
  import { getApp } from '$lib/context'
  import { cueColor, formatClock, hotCueLetter } from '$lib/format'
  import type { BrowserWave } from '$lib/browserPreview.svelte'
  import type { Cue, Track } from '$lib/types'
  import { drawAnalysisRgb, drawStackedBands } from '$lib/waveform/analysis'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { LOOP_ACTIVE_COLOR } from '$lib/waveform/colors'

  let { track, wave, cues }: { track: Track; wave: BrowserWave | null; cues: Cue[] } = $props()

  const { player, audition } = getApp()

  let width = $state(0)
  let height = $state(0)
  let hover = $state<number | null>(null)
  let holding = $state(false)

  const span = $derived(track.duration || 1)
  const head = $derived(audition.trackId === track.id ? audition.position : null)
  const loop = $derived(audition.trackId === track.id ? audition.loop : null)
  const line = $derived(holding ? head : (hover ?? head))

  function releaseOnRemove(): () => void {
    return () => {
      if (holding) audition.stop()
    }
  }

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || !height || !wave) return
    const ctx = setupCanvas(canvas, width, height)
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    const bands = player.display.mode === '3band' ? wave.bands : null
    if (bands) drawStackedBands(ctx, bands, canvas.width, canvas.height)
    else if (wave.heights && wave.colors)
      drawAnalysisRgb(ctx, wave.heights, wave.colors, 0, wave.rgbLength, canvas.width, canvas.height, 'bottom')
    else if (wave.bands) drawStackedBands(ctx, wave.bands, canvas.width, canvas.height)
  }

  function secondsAt(e: PointerEvent): number {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left))
    return (x / Math.max(rect.width, 1)) * span
  }

  function pct(seconds: number): number {
    return Math.max(0, Math.min(100, (seconds / span) * 100))
  }

  function cueLabel(cue: Cue): string {
    const name = cue.kind === 'hot' ? `Hot cue ${hotCueLetter(cue.slot)}` : 'Memory cue'
    const time = formatClock(cue.start)
    return cue.comment ? `${name}, ${time}, ${cue.comment}` : `${name}, ${time}`
  }
</script>

{#if wave}
  <div
    class="relative h-[85%] max-h-[85%] w-full min-w-0 cursor-pointer overflow-hidden rounded-[3px] bg-black/40"
    {@attach releaseOnRemove}
    bind:clientWidth={width}
    bind:clientHeight={height}
    role="button"
    tabindex="-1"
    aria-label="Preview {track.title}"
    aria-pressed={holding}
    title="Hold to preview"
    onmousedown={(e) => {
      e.preventDefault()
      e.stopPropagation()
    }}
    ondblclick={(e) => e.stopPropagation()}
    ondragstart={(e) => {
      e.preventDefault()
      e.stopPropagation()
    }}
    onpointerdown={(e) => {
      if (e.button !== 0) return
      e.stopPropagation()
      e.currentTarget.setPointerCapture(e.pointerId)
      holding = true
      hover = null
      audition.playAt(track, secondsAt(e))
    }}
    onpointermove={(e) => {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) return
      hover = secondsAt(e)
    }}
    onpointerup={(e) => {
      if (!holding) return
      holding = false
      audition.stop()
      const rect = e.currentTarget.getBoundingClientRect()
      const inside =
        e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom
      hover = inside ? secondsAt(e) : null
    }}
    onpointerleave={() => {
      if (!holding) hover = null
    }}
    onpointercancel={() => {
      if (!holding) return
      holding = false
      hover = null
      audition.stop()
    }}
  >
    <canvas {@attach paint} draggable="false" class="pointer-events-none absolute inset-0 size-full" aria-hidden="true"
    ></canvas>
    {#if head != null}
      <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/45" style:width="{pct(head)}%"></div>
    {/if}
    {#if loop}
      <div
        class={['pointer-events-none absolute inset-y-0 border-x', loop.active ? 'opacity-40' : 'opacity-15']}
        style:left="{pct(loop.start)}%"
        style:width="{pct(loop.end) - pct(loop.start)}%"
        style:background={LOOP_ACTIVE_COLOR}
        style:border-color={LOOP_ACTIVE_COLOR}
      ></div>
    {/if}
    {#each cues as cue (cue.id)}
      {#if cue.end != null}
        <div
          class="pointer-events-none absolute inset-y-0 opacity-25"
          style:left="{pct(cue.start / 1000)}%"
          style:width="{pct(cue.end / 1000) - pct(cue.start / 1000)}%"
          style:background={cueColor(cue)}
        ></div>
      {/if}
      <button
        type="button"
        class={[
          'absolute cursor-pointer border-0 bg-transparent p-0 hover:brightness-125',
          cue.kind === 'hot'
            ? 'top-0 z-20 grid size-3 place-items-center text-[8px] leading-none font-bold text-black/85'
            : 'top-0 z-10 flex h-4 w-4 -translate-x-1/2 items-start justify-center'
        ]}
        style:left="{pct(cue.start / 1000)}%"
        style:background={cue.kind === 'hot' ? cueColor(cue) : undefined}
        title={cueLabel(cue)}
        aria-label={cueLabel(cue)}
        draggable="false"
        onpointerdown={(e) => {
          if (e.button !== 0) return
          e.stopPropagation()
          e.preventDefault()
          e.currentTarget.setPointerCapture(e.pointerId)
          holding = true
          hover = null
          audition.playCue(track, cue)
        }}
        onpointerup={(e) => {
          e.stopPropagation()
          if (!holding) return
          holding = false
          audition.stop()
        }}
        onpointercancel={(e) => {
          e.stopPropagation()
          if (!holding) return
          holding = false
          audition.stop()
        }}
        onclick={(e) => e.stopPropagation()}
      >
        {#if cue.kind === 'hot'}
          {hotCueLetter(cue.slot)}
        {:else}
          <span
            class="pointer-events-none h-[7px] w-[9px]"
            style:background={cueColor(cue)}
            style:clip-path="polygon(0 0, 100% 0, 50% 100%)"
          ></span>
        {/if}
      </button>
    {/each}
    {#if line != null}
      <div
        class="pointer-events-none absolute inset-y-0 w-px -translate-x-1/2 bg-white"
        style:left="{pct(line)}%"
      ></div>
    {/if}
  </div>
{/if}
