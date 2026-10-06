<script lang="ts">
  import { getApp } from '$lib/context'
  import { cueColor, drawOverview, setupCanvas } from '$lib/waveform'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)

  const span = $derived(player.waveform ? player.waveform.length / player.waveform.rate : player.duration || 1)
  const pct = (seconds: number): number => Math.max(0, Math.min(100, (seconds / span) * 100))

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || !height) return
    const ctx = setupCanvas(canvas, width, height)
    if (player.waveform) drawOverview(ctx, player.waveform, player.mode, canvas.width, canvas.height)
    else ctx.clearRect(0, 0, canvas.width, canvas.height)
  }

  let wheelTimer = 0

  function place(e: PointerEvent): void {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left))
    player.scrub((x / rect.width) * span)
  }

  function onwheel(e: WheelEvent): void {
    if (!player.waveform || !width) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    player.scrub(player.position + delta * (span / width))
    clearTimeout(wheelTimer)
    wheelTimer = window.setTimeout(() => player.commit(), 80)
  }
</script>

<div
  class="relative h-full w-full cursor-pointer overflow-hidden rounded-md bg-black/30"
  bind:clientWidth={width}
  bind:clientHeight={height}
  role="slider"
  tabindex="-1"
  aria-label="Track position"
  aria-valuemin={0}
  aria-valuemax={Math.round(span)}
  aria-valuenow={Math.round(player.position)}
  {onwheel}
  onpointerdown={(e) => {
    e.currentTarget.setPointerCapture(e.pointerId)
    place(e)
  }}
  onpointermove={(e) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) place(e)
  }}
  onpointerup={() => player.commit()}
  onpointercancel={() => player.commit()}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
  <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/50" style:width="{pct(player.position)}%"></div>

  {#each player.detail?.cues ?? [] as cue (cue.id)}
    {#if cue.end != null}
      <div
        class="pointer-events-none absolute inset-y-0 opacity-25"
        style:left="{pct(cue.start / 1000)}%"
        style:width="{pct(cue.end / 1000) - pct(cue.start / 1000)}%"
        style:background={cueColor(cue)}
      ></div>
    {/if}
    <span
      class={[
        'pointer-events-none absolute h-[7px] w-[9px] -translate-x-1/2',
        cue.kind === 'hot' ? 'bottom-0' : 'top-0'
      ]}
      style:left="{pct(cue.start / 1000)}%"
      style:background={cueColor(cue)}
      style:clip-path={cue.kind === 'hot'
        ? 'polygon(50% 0, 0 100%, 100% 100%)'
        : 'polygon(0 0, 100% 0, 50% 100%)'}
    ></span>
  {/each}

  <div
    class="pointer-events-none absolute inset-y-0 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_6px_rgba(255,255,255,0.6)]"
    style:left="{pct(player.position)}%"
  ></div>
</div>
