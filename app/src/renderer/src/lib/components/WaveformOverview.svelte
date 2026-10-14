<script lang="ts">
  import { getApp } from '$lib/context'
  import { cueColor, formatClock, hotCueLetter } from '$lib/format'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { CUE_POINT_COLOR, LOOP_ACTIVE_COLOR } from '$lib/waveform/colors'
  import { drawOverview } from '$lib/waveform/overview'
  import type { Cue } from '$lib/types'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)

  const span = $derived(player.waveform ? player.waveform.length / player.waveform.rate : player.duration || 1)
  const pct = (seconds: number): number => Math.max(0, Math.min(100, (seconds / span) * 100))
  const lineAt = $derived(player.previewAt ?? player.position)

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || !height) return
    const ctx = setupCanvas(canvas, width, height)
    if (player.waveform)
      drawOverview(
        ctx,
        player.waveform,
        player.display.mode,
        canvas.width,
        canvas.height,
        player.display.prepared ? player.display.samples : null
      )
    else ctx.clearRect(0, 0, canvas.width, canvas.height)
  }

  const WAVEFORM_PREVIEW = 'waveform'
  let rawWheelSeconds: number | null = null

  function secondsAt(e: PointerEvent): number {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left))
    return player.quantised((x / Math.max(rect.width, 1)) * span)
  }

  function drag(e: PointerEvent): void {
    rawWheelSeconds = null
    player.scrub(secondsAt(e), e.timeStamp)
  }

  function preview(e: PointerEvent): void {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const inside =
      e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom
    const overCue = e.target instanceof Element && e.target.closest('button')
    if (inside && !overCue) player.setPreview(WAVEFORM_PREVIEW, secondsAt(e))
    else player.clearPreview(WAVEFORM_PREVIEW)
  }

  function cueLabel(cue: Cue): string {
    const name = cue.kind === 'hot' ? `Hot cue ${hotCueLetter(cue.slot)}` : 'Memory cue'
    const time = formatClock(cue.start)
    return cue.comment ? `${name}, ${time}, ${cue.comment}` : `${name}, ${time}`
  }

  function onwheel(e: WheelEvent): void {
    if (!player.waveform || !width) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    const at = Math.max(0, Math.min(span, (rawWheelSeconds ?? player.position) + delta * (span / width)))
    rawWheelSeconds = at
    player.seek(player.quantised(at))
  }
</script>

<div
  class="relative h-full w-full cursor-ew-resize overflow-hidden bg-black/30"
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
    player.clearPreview(WAVEFORM_PREVIEW)
    e.currentTarget.setPointerCapture(e.pointerId)
    rawWheelSeconds = null
    player.seek(secondsAt(e))
  }}
  onpointermove={(e) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) drag(e)
    else preview(e)
  }}
  onpointerleave={() => player.clearPreview(WAVEFORM_PREVIEW)}
  onpointerup={(e) => {
    player.endScrub()
    preview(e)
  }}
  onpointercancel={() => {
    player.clearPreview(WAVEFORM_PREVIEW)
    player.endScrub()
  }}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
  <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/50" style:width="{pct(player.position)}%"></div>

  {#if player.loop}
    <div
      class={['pointer-events-none absolute inset-y-0 border-x', player.loop.active ? 'opacity-40' : 'opacity-15']}
      style:left="{pct(player.loop.start)}%"
      style:width="{pct(player.loop.end) - pct(player.loop.start)}%"
      style:background={LOOP_ACTIVE_COLOR}
      style:border-color={LOOP_ACTIVE_COLOR}
    ></div>
  {/if}

  {#each player.displayCues as cue (cue.id)}
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
      onpointerdown={(e) => e.stopPropagation()}
      onclick={() => (cue.kind === 'memory' ? player.callCue(cue) : player.seek(cue.start / 1000))}
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

  {#if player.track}
    <span
      class="pointer-events-none absolute bottom-0 z-10 h-[7px] w-[9px] -translate-x-1/2"
      style:left="{pct(player.cuePoint)}%"
      style:background={CUE_POINT_COLOR}
      style:clip-path="polygon(50% 0, 100% 100%, 0 100%)"
    ></span>
  {/if}

  <div
    class="pointer-events-none absolute inset-y-0 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_6px_rgba(255,255,255,0.6)]"
    style:left="{pct(lineAt)}%"
  ></div>
</div>
