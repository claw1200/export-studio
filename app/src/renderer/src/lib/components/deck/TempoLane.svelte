<script lang="ts">
  import { BEATS_PER_BAR, nearestBeatIndex } from '$lib/beat'
  import { getPlayer } from '$lib/deck/context'
  import { formatBeatLength, formatBpm, formatClock } from '$lib/format'
  import {
    continuedBeats,
    type CompiledGrid,
    endBpm,
    MIN_RAMP_BEATS,
    rampEndMs,
    sectionEnd,
    type TempoMap,
    varies
  } from '$lib/tempo-map'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { TEMPO_FLAG_COLOR } from '$lib/waveform/colors'
  import FlagPopover from './FlagPopover.svelte'

  const player = getPlayer()
  const grid = player.grid

  const DRAG_THRESHOLD_PX = 3
  const RAMP_HANDLE_HIT_PX = 6
  const SNAP_LOOKAHEAD_MS = 2000
  const RAMP_HANDLE_RADIUS = 5
  const MIN_TEMPO_PAD_BPM = 4
  const TEMPO_PAD_FRACTION = 0.2

  type Target = { kind: 'flag' | 'ramp'; flagIndex: number }
  type FlagShape = { flagIndex: number; left: number; right: number; rampX: number | null }

  let width = $state(0)
  let height = $state(0)
  let hover = $state.raw<Target | null>(null)
  let dragging = $state<Target['kind'] | null>(null)
  let drag: (Target & { x: number; dragStart: TempoMap; moved: boolean }) | null = null
  let flagShapes: FlagShape[] = []

  const pxPerSecond = (): number => (player.waveform?.rate ?? 0) * player.display.zoom
  const playheadX = (): number => width * player.display.playheadAt
  const xOf = (ms: number): number => playheadX() + (ms / 1000 - player.position) * pxPerSecond()
  const msAt = (x: number): number => (player.position + (x - playheadX()) / Math.max(pxPerSecond(), 1e-6)) * 1000
  const localX = (e: MouseEvent): number => e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left
  const nearest = (values: number[], target: number): number =>
    values.reduce((best, value) => (Math.abs(value - target) < Math.abs(best - target) ? value : best))
  const isHovered = (kind: Target['kind'], flagIndex: number): boolean =>
    hover?.kind === kind && hover.flagIndex === flagIndex

  function flagLabel(map: TempoMap, flagIndex: number): string {
    const flag = map.flags[flagIndex]
    const bpm = formatBpm(flag.bpm)
    if (varies(flag)) return `~${bpm}`
    if (flagIndex === 0 || !flag.rampBeats) return bpm
    return `${flag.bpm >= endBpm(map.flags[flagIndex - 1]) ? '↗' : '↘'} ${bpm}`
  }

  function describeTarget(map: TempoMap, target: Target | null): string {
    if (!target) return 'Double-click to add a tempo flag on the nearest beat'
    if (target.kind === 'ramp') return 'Drag to set the ramp length in bars (Shift: single beats)'
    const { flagIndex } = target
    const flag = map.flags[flagIndex]
    const tempo = varies(flag)
      ? `Tempo varies around ${formatBpm(flag.bpm)} BPM (rekordbox analysis)`
      : flagIndex > 0 && flag.rampBeats
        ? `${formatBpm(endBpm(map.flags[flagIndex - 1]))} → ${formatBpm(flag.bpm)} BPM over ${formatBeatLength(flag.rampBeats)}`
        : `${formatBpm(flag.bpm)} BPM`
    const dragHint = flagIndex === 0 ? 'drag to move the first section' : 'drag to move it (Alt: off the grid)'
    return `${tempo}, from ${formatClock(flag.startMs)}. Click for options, ${dragHint}.`
  }

  function paint(canvas: HTMLCanvasElement): void {
    const { map, draft } = grid
    if (!width || !height || !map || !draft) return
    const dpr = window.devicePixelRatio || 1
    const ctx = setupCanvas(canvas, width, height)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, width, height)
    const tempoY = tempoScale(map)
    paintDownbeats(ctx, draft)
    paintTempoLine(ctx, map, draft, tempoY)
    flagShapes = paintFlags(ctx, map, draft, tempoY)
  }

  function tempoScale(map: TempoMap): (bpm: number) => number {
    const bpms = map.flags.map((flag) => flag.bpm)
    const lo = Math.min(...bpms)
    const hi = Math.max(...bpms)
    const pad = Math.max(MIN_TEMPO_PAD_BPM, (hi - lo) * TEMPO_PAD_FRACTION)
    return (bpm) => height - 4 - ((bpm - lo + pad) / (hi - lo + 2 * pad)) * (height - 8)
  }

  function paintDownbeats(ctx: CanvasRenderingContext2D, draft: CompiledGrid): void {
    const fromMs = msAt(0)
    const toMs = msAt(width)
    ctx.fillStyle = 'rgba(255,255,255,0.08)'
    draft.times.forEach((time, i) => {
      if (time >= fromMs && time <= toMs && draft.beats[i] === 1) ctx.fillRect(Math.round(xOf(time)), height - 3, 1, 3)
    })
  }

  function paintTempoLine(
    ctx: CanvasRenderingContext2D,
    map: TempoMap,
    draft: CompiledGrid,
    tempoY: (bpm: number) => number
  ): void {
    const points: [number, number][] = []
    map.flags.forEach((flag, flagIndex) => {
      const sectionEndX = xOf(map.flags[flagIndex + 1]?.startMs ?? map.durationMs)
      if (flagIndex === 0) points.push([Math.min(xOf(0), xOf(flag.startMs)), tempoY(flag.bpm)])
      else {
        const rampEnd = rampEndMs(draft, map, flagIndex)
        const reachedX = !flag.rampBeats ? xOf(flag.startMs) : rampEnd == null ? sectionEndX : xOf(rampEnd)
        points.push([xOf(flag.startMs), tempoY(endBpm(map.flags[flagIndex - 1]))], [reachedX, tempoY(flag.bpm)])
      }
      points.push([sectionEndX, tempoY(flag.bpm)])
    })
    const trace = (): void => {
      ctx.beginPath()
      points.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)))
    }
    trace()
    ctx.lineTo(points[points.length - 1][0], height)
    ctx.lineTo(points[0][0], height)
    ctx.fillStyle = TEMPO_FLAG_COLOR + '1a'
    ctx.fill()
    trace()
    ctx.strokeStyle = TEMPO_FLAG_COLOR + 'b3'
    ctx.lineWidth = 1.25
    ctx.lineJoin = 'round'
    ctx.stroke()
  }

  function paintRampHandle(ctx: CanvasRenderingContext2D, x: number, y: number, hot: boolean): void {
    ctx.beginPath()
    const r = RAMP_HANDLE_RADIUS
    ctx.moveTo(x, y - r)
    ctx.lineTo(x + r, y)
    ctx.lineTo(x, y + r)
    ctx.lineTo(x - r, y)
    ctx.closePath()
    ctx.fillStyle = hot ? '#ffffff' : TEMPO_FLAG_COLOR
    ctx.fill()
    ctx.strokeStyle = 'rgba(0,0,0,0.6)'
    ctx.lineWidth = 1
    ctx.stroke()
  }

  function paintFlags(
    ctx: CanvasRenderingContext2D,
    map: TempoMap,
    draft: CompiledGrid,
    tempoY: (bpm: number) => number
  ): FlagShape[] {
    ctx.font = '650 10px "Inter Variable", system-ui, sans-serif'
    ctx.textBaseline = 'middle'
    ctx.textAlign = 'left'
    const pennantTop = 3
    const pennantHeight = height - 7
    let previousRight = -Infinity
    return map.flags.map((flag, flagIndex) => {
      const x = Math.round(xOf(flag.startMs)) + 0.5
      const rampEnd = rampEndMs(draft, map, flagIndex)
      const rampX = rampEnd == null ? null : xOf(rampEnd)
      if (rampX != null && rampX > -8 && rampX < width + 8)
        paintRampHandle(ctx, rampX, tempoY(flag.bpm), isHovered('ramp', flagIndex))
      const label = flagLabel(map, flagIndex)
      const pennantWidth = Math.ceil(ctx.measureText(label).width) + 9
      const labelFits = x > previousRight + 2
      const shape = { flagIndex, left: x - 4, right: labelFits ? x + pennantWidth : x + 4, rampX }
      if (x < -pennantWidth - 4 || x > width + 4) return shape
      ctx.fillStyle = TEMPO_FLAG_COLOR
      ctx.fillRect(x - 0.5, 0, 1, height)
      if (!labelFits) return shape
      const selected = grid.selectedFlagId === flag.id
      ctx.beginPath()
      ctx.roundRect(x + 0.5, pennantTop, pennantWidth, pennantHeight, [0, 3, 3, 0])
      ctx.fillStyle = selected ? TEMPO_FLAG_COLOR : TEMPO_FLAG_COLOR + (isHovered('flag', flagIndex) ? '6b' : '42')
      ctx.fill()
      ctx.fillStyle = selected ? '#16101f' : '#f3edff'
      ctx.fillText(label, x + 5, pennantTop + pennantHeight / 2 + 0.5)
      previousRight = x + pennantWidth
      return shape
    })
  }

  function targetAt(x: number): Target | null {
    const ramp = flagShapes.find((s) => s.rampX != null && Math.abs(x - s.rampX) <= RAMP_HANDLE_HIT_PX)
    if (ramp) return { kind: 'ramp', flagIndex: ramp.flagIndex }
    const flag = flagShapes.findLast((s) => x >= s.left && x <= s.right)
    return flag ? { kind: 'flag', flagIndex: flag.flagIndex } : null
  }

  function rampBeatsAt(draft: CompiledGrid, flagIndex: number, ms: number, exact: boolean): number {
    const start = draft.sectionStart[flagIndex]
    const lastStep = sectionEnd(draft, flagIndex) - 1 - start
    const step = Math.max(0, Math.min(lastStep, nearestBeatIndex(draft.times, ms) - start))
    const beats = exact ? step : Math.round(step / BEATS_PER_BAR) * BEATS_PER_BAR
    return Math.max(MIN_RAMP_BEATS, Math.min(Math.max(MIN_RAMP_BEATS, lastStep), beats))
  }

  function snappedFlagMs(flagIndex: number, ms: number): number {
    const draft = grid.draft
    return draft ? nearest(continuedBeats(draft, flagIndex - 1, ms + SNAP_LOOKAHEAD_MS), ms) : ms
  }

  function onpointermove(e: PointerEvent): void {
    const x = localX(e)
    if (!drag) {
      const target = targetAt(x)
      if (target?.kind !== hover?.kind || target?.flagIndex !== hover?.flagIndex) hover = target
      return
    }
    if (!drag.moved && Math.abs(e.clientX - drag.x) < DRAG_THRESHOLD_PX) return
    drag.moved = true
    dragging = drag.kind
    const { dragStart, flagIndex } = drag
    if (drag.kind === 'ramp') {
      if (grid.draft) grid.setRamp(flagIndex, rampBeatsAt(grid.draft, flagIndex, msAt(x), e.shiftKey), dragStart)
    } else if (flagIndex === 0) grid.shiftSection(0, ((e.clientX - drag.x) / pxPerSecond()) * 1000, dragStart)
    else grid.moveFlag(flagIndex, e.altKey ? msAt(x) : snappedFlagMs(flagIndex, msAt(x)), dragStart)
  }

  function endDrag(): void {
    const ended = drag
    drag = null
    dragging = null
    if (!ended || ended.moved || ended.kind !== 'flag') return
    const id = grid.map?.flags[ended.flagIndex]?.id ?? null
    grid.select(grid.selectedFlagId === id ? null : id)
  }

  const cursor = $derived.by(() => {
    if (dragging) return dragging === 'flag' ? 'cursor-grabbing' : 'cursor-ew-resize'
    if (hover) return hover.kind === 'ramp' ? 'cursor-ew-resize' : 'cursor-grab'
    return 'cursor-default'
  })
</script>

<div
  class="relative h-[22px] border-b border-line-soft bg-[oklch(0.14_0.007_290)]"
  bind:clientWidth={width}
  bind:clientHeight={height}
>
  <div
    data-tempo-lane
    class={['absolute inset-0 touch-none', cursor]}
    role="toolbar"
    tabindex="-1"
    aria-label="Tempo flags. Double-click to add a flag."
    title={dragging || !grid.map ? undefined : describeTarget(grid.map, hover)}
    onpointerdown={(e) => {
      const target = targetAt(localX(e))
      if (e.button !== 0 || !grid.map || !target) return
      e.currentTarget.setPointerCapture(e.pointerId)
      drag = { ...target, x: e.clientX, dragStart: grid.map, moved: false }
    }}
    {onpointermove}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onpointerleave={() => {
      if (!drag) hover = null
    }}
    ondblclick={(e) => {
      if (!targetAt(localX(e))) grid.addFlagAt(msAt(localX(e)))
    }}
  >
    <canvas {@attach paint} class="pointer-events-none absolute inset-0 size-full"></canvas>
  </div>
  <FlagPopover {xOf} {width} />
</div>
