<script lang="ts">
  import { getPlayer } from '$lib/deck/context'
  import { cueSections, formatBars, type CueSection } from '$lib/deck/sections'
  import { formatClock } from '$lib/format'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { CUE_POINT_COLOR } from '$lib/waveform/colors'
  import { drawOverview } from '$lib/waveform/overview'
  import WaveformMarks from './WaveformMarks.svelte'

  /**
   * `sections` adds a strip under the waveform with a segment for each stretch between cue points,
   * labelled with its length in bars. The segment the playhead is in is lit.
   */
  let { sections = false }: { sections?: boolean } = $props()

  const player = getPlayer()
  /** Narrower than this, in pixels, a section's bar count does not fit and is left off. */
  const LABEL_MIN_WIDTH = 18
  /** Height of the section strip, in pixels. It sits below the waveform, outside the part that seeks. */
  const STRIP = 15

  let width = $state(0)
  /** Height of the waveform alone, above the section strip. */
  let height = $state(0)

  const span = $derived(player.waveform ? player.waveform.length / player.waveform.rate : player.duration || 1)
  const pct = (seconds: number): number => Math.max(0, Math.min(100, (seconds / span) * 100))
  const lineAt = $derived(player.previewAt ?? player.position)
  const bands = $derived(
    sections && player.track
      ? cueSections(player.displayCues, player.displayGrid, player.track.bpm ?? 0, player.duration || span)
      : []
  )
  const bandWidth = (band: { start: number; end: number }): number => ((pct(band.end) - pct(band.start)) / 100) * width
  /** Owns the hover line while a section is pointed at, apart from the waveform's own hover. */
  const SECTION_PREVIEW = 'section'

  function sectionLabel(band: CueSection): string {
    if (band.edge === 'in') return 'In'
    if (band.edge === 'out') return 'Out'
    return band.bars != null ? formatBars(band.bars) : ''
  }

  function sectionTitle(band: CueSection): string {
    const length = band.bars != null ? `${formatBars(band.bars)} bars` : null
    const name = band.edge === 'in' ? 'Intro' : band.edge === 'out' ? 'Outro' : null
    const what = [name, length].filter(Boolean).join(', ') || 'Section'
    return `${what}. Click to go to ${formatClock(band.start * 1000)}`
  }

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || height <= 0) return
    const ctx = setupCanvas(canvas, width, height)
    if (player.waveform)
      drawOverview(
        ctx,
        player.waveform,
        player.display.mode,
        canvas.width,
        canvas.height,
        player.display.prepared ? player.display.samples : null,
        player.display.rgbColors(player.waveform),
        player.display.heightScales.overview
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

  function onwheel(e: WheelEvent): void {
    if (!player.waveform || !width) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    const at = Math.max(0, Math.min(span, (rawWheelSeconds ?? player.position) + delta * (span / width)))
    rawWheelSeconds = at
    player.seek(player.quantised(at))
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-black/30">
  <div
    class="relative min-h-0 w-full flex-1 cursor-ew-resize"
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
    <canvas {@attach paint} class="absolute inset-0 h-full w-full"></canvas>
    <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/50" style:width="{pct(player.position)}%"></div>

    <WaveformMarks
      cues={player.displayCues}
      loop={player.loop}
      {pct}
      onpointerdown={(_cue, e) => e.stopPropagation()}
      onclick={(cue) => (cue.kind === 'memory' ? player.callCue(cue) : player.seek(cue.start / 1000))}
    />

    {#if player.grid.editing}
      {#each player.grid.flagMarks as flag (flag.startMs)}
        <span class="pointer-events-none absolute inset-y-0 w-px bg-flag/80" style:left="{pct(flag.startMs / 1000)}%"
        ></span>
        <span
          class="pointer-events-none absolute top-0 h-[6px] w-[6px] bg-flag"
          style:left="{pct(flag.startMs / 1000)}%"
          style:clip-path="polygon(0 0, 100% 0, 0 100%)"
        ></span>
      {/each}
    {/if}

    {#if player.track}
      <span
        class="pointer-events-none absolute bottom-0 z-10 h-[7px] w-[9px] -translate-x-1/2"
        style:left="{pct(player.cuePoint)}%"
        style:background={CUE_POINT_COLOR}
        style:clip-path="polygon(50% 0, 100% 100%, 0 100%)"
      ></span>
    {/if}

    <div
      class="pointer-events-none absolute inset-y-0 z-20 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_6px_rgba(255,255,255,0.6)]"
      style:left="{pct(lineAt)}%"
    ></div>
  </div>

  {#if bands.length}
    <!-- Its own row under the waveform, so pointing between or around sections never seeks. -->
    <div class="relative w-full shrink-0" style:height="{STRIP}px">
      {#each bands as band (band.start)}
        {@const current = player.position >= band.start && player.position < band.end}
        {@const played = player.position >= band.end}
        <button
          class={[
            'tabular absolute bottom-px h-[13px] cursor-pointer overflow-hidden rounded-[3px] text-center text-[9.5px] leading-[13px] font-semibold transition-colors',
            current
              ? 'bg-accent text-bg'
              : played
                ? 'bg-white/[0.06] text-faint hover:bg-white/[0.2] hover:text-fg'
                : 'bg-white/[0.13] text-muted hover:bg-white/[0.24] hover:text-fg'
          ]}
          style:left="calc({pct(band.start)}% + 1px)"
          style:width="calc({pct(band.end) - pct(band.start)}% - 2px)"
          title={sectionTitle(band)}
          aria-label={sectionTitle(band)}
          onpointerenter={() => player.setPreview(SECTION_PREVIEW, band.start)}
          onpointerleave={() => player.clearPreview(SECTION_PREVIEW)}
          onclick={() => {
            player.clearPreview(SECTION_PREVIEW)
            player.cueAt(band.start)
          }}
        >
          {#if bandWidth(band) >= LABEL_MIN_WIDTH}{sectionLabel(band)}{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
