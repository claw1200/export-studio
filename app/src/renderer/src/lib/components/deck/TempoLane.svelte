<script lang="ts">
  import { getApp } from '$lib/context'
  import { formatBpm, formatClock } from '$lib/format'
  import { continuedBeats, endBpm, MIN_RAMP, rampEndMs, type TempoMap, varies } from '$lib/tempo-map'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { TEMPO_FLAG_COLOR } from '$lib/waveform/colors'
  import FlagPopover from './FlagPopover.svelte'

  /** The strip under the waveform that holds the tempo flags while the grid is edited. */

  const { player } = getApp()
  const grid = player.grid

  let width = $state(0)
  let height = $state(0)

  type Hit = { kind: 'flag' | 'ramp'; j: number } | null
  /** Where each flag's pennant and ramp handle were last drawn, in CSS pixels. */
  let shapes: { j: number; left: number; right: number; rampX: number | null }[] = []
  let hover = $state.raw<Hit>(null)
  let drag: {
    kind: 'flag' | 'ramp'
    j: number
    x: number
    base: TempoMap
    moved: boolean
  } | null = null
  let dragging = $state<'flag' | 'ramp' | null>(null)

  const pxPerSecond = (): number => (player.waveform?.rate ?? 0) * player.display.zoom
  const xOf = (ms: number): number => width / 2 + (ms / 1000 - player.position) * pxPerSecond()
  const msAt = (x: number): number => (player.position + (x - width / 2) / Math.max(pxPerSecond(), 1e-6)) * 1000

  function label(j: number): string {
    const map = grid.map
    if (!map) return ''
    const flag = map.flags[j]
    const bpm = formatBpm(flag.bpm)
    if (varies(flag)) return `~${bpm}`
    if (j > 0 && flag.ramp) return `${flag.bpm >= endBpm(map.flags[j - 1]) ? '↗' : '↘'} ${bpm}`
    return bpm
  }

  function paint(canvas: HTMLCanvasElement): void {
    const map = grid.map
    const draft = grid.draft
    if (!width || !height || !map || !draft) return
    const dpr = window.devicePixelRatio || 1
    const ctx = setupCanvas(canvas, width, height)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, width, height)
    void player.position
    void player.display.zoom

    // Downbeat ticks, so snapping a flag is predictable.
    const fromMs = msAt(0)
    const toMs = msAt(width)
    ctx.fillStyle = 'rgba(255,255,255,0.08)'
    for (let i = 0; i < draft.times.length; i++) {
      const t = draft.times[i]
      if (t < fromMs || t > toMs || draft.beats[i] !== 1) continue
      ctx.fillRect(Math.round(xOf(t)), height - 3, 1, 3)
    }

    // The tempo line: flat while steady, sloped through ramps, stepped at jumps.
    let lo = Infinity
    let hi = -Infinity
    for (const f of map.flags) {
      lo = Math.min(lo, f.bpm)
      hi = Math.max(hi, f.bpm)
    }
    const pad = Math.max(4, (hi - lo) * 0.2)
    const y = (bpm: number): number => height - 4 - ((bpm - lo + pad) / (hi - lo + 2 * pad)) * (height - 8)
    const points: [number, number][] = []
    map.flags.forEach((f, j) => {
      const end = xOf(j + 1 < map.flags.length ? map.flags[j + 1].ms : map.durationMs)
      if (j === 0) points.push([Math.min(xOf(0), xOf(f.ms)), y(f.bpm)])
      else {
        points.push([xOf(f.ms), y(endBpm(map.flags[j - 1]))])
        const rampEnd = rampEndMs(draft, map, j)
        points.push([f.ramp ? (rampEnd == null ? end : xOf(rampEnd)) : xOf(f.ms), y(f.bpm)])
      }
      points.push([end, y(f.bpm)])
    })
    ctx.beginPath()
    points.forEach(([px, py], i) => (i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)))
    ctx.lineTo(points[points.length - 1][0], height)
    ctx.lineTo(points[0][0], height)
    ctx.closePath()
    ctx.fillStyle = TEMPO_FLAG_COLOR + '1a'
    ctx.fill()
    ctx.beginPath()
    points.forEach(([px, py], i) => (i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)))
    ctx.strokeStyle = TEMPO_FLAG_COLOR + 'b3'
    ctx.lineWidth = 1.25
    ctx.lineJoin = 'round'
    ctx.stroke()

    // Flags: a pole and a pennant with the tempo; ramps end in a diamond handle.
    ctx.font = '650 10px "Inter Variable", system-ui, sans-serif'
    ctx.textBaseline = 'middle'
    ctx.textAlign = 'left'
    const next: typeof shapes = []
    let lastRight = -Infinity
    map.flags.forEach((f, j) => {
      const x = Math.round(xOf(f.ms)) + 0.5
      const rampEnd = rampEndMs(draft, map, j)
      const rampX = rampEnd == null ? null : xOf(rampEnd)
      if (rampX != null && rampX > -8 && rampX < width + 8) {
        const ry = y(f.bpm)
        const hot = hover?.kind === 'ramp' && hover.j === j
        ctx.beginPath()
        ctx.moveTo(rampX, ry - 5)
        ctx.lineTo(rampX + 5, ry)
        ctx.lineTo(rampX, ry + 5)
        ctx.lineTo(rampX - 5, ry)
        ctx.closePath()
        ctx.fillStyle = hot ? '#ffffff' : TEMPO_FLAG_COLOR
        ctx.fill()
        ctx.strokeStyle = 'rgba(0,0,0,0.6)'
        ctx.lineWidth = 1
        ctx.stroke()
      }
      const text = label(j)
      const w = Math.ceil(ctx.measureText(text).width) + 9
      const fits = x > lastRight + 2
      next.push({ j, left: x - 4, right: fits ? x + w : x + 4, rampX })
      if (x < -w - 4 || x > width + 4) return
      ctx.fillStyle = TEMPO_FLAG_COLOR
      ctx.fillRect(x - 0.5, 0, 1, height)
      if (!fits) return
      const selected = grid.selected === f.id
      const hot = hover?.kind === 'flag' && hover.j === j
      ctx.beginPath()
      ctx.roundRect(x + 0.5, 3, w, height - 7, [0, 3, 3, 0])
      ctx.fillStyle = selected ? TEMPO_FLAG_COLOR : TEMPO_FLAG_COLOR + (hot ? '6b' : '42')
      ctx.fill()
      ctx.fillStyle = selected ? '#16101f' : '#f3edff'
      ctx.fillText(text, x + 5, 3 + (height - 7) / 2 + 0.5)
      lastRight = x + w
    })
    shapes = next
  }

  function hitAt(x: number): Hit {
    for (const s of shapes) if (s.rampX != null && Math.abs(x - s.rampX) <= 6) return { kind: 'ramp', j: s.j }
    for (let i = shapes.length - 1; i >= 0; i--)
      if (x >= shapes[i].left && x <= shapes[i].right) return { kind: 'flag', j: shapes[i].j }
    return null
  }

  const localX = (e: PointerEvent | MouseEvent): number =>
    e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left

  function hint(hit: Hit): string {
    const map = grid.map
    if (!hit || !map) return 'Double-click to add a tempo flag on the nearest beat'
    const flag = map.flags[hit.j]
    if (hit.kind === 'ramp') return 'Drag to set the ramp length in bars (Shift: single beats)'
    const what = varies(flag)
      ? `Tempo varies around ${formatBpm(flag.bpm)} BPM (rekordbox analysis)`
      : hit.j > 0 && flag.ramp
        ? `${formatBpm(endBpm(map.flags[hit.j - 1]))} → ${formatBpm(flag.bpm)} BPM over ${flag.ramp % 4 ? `${flag.ramp} beats` : `${flag.ramp / 4} bars`}`
        : `${formatBpm(flag.bpm)} BPM`
    const move = hit.j === 0 ? 'drag to move the first section' : 'drag to move it (Alt: off the grid)'
    return `${what}, from ${formatClock(flag.ms)}. Click for options, ${move}.`
  }

  function onpointermove(e: PointerEvent): void {
    const x = localX(e)
    if (!drag) {
      const hit = hitAt(x)
      if (hit?.kind !== hover?.kind || hit?.j !== hover?.j) hover = hit
      return
    }
    if (!drag.moved && Math.abs(e.clientX - drag.x) < 3) return
    drag.moved = true
    dragging = drag.kind
    const { base, j } = drag
    if (drag.kind === 'ramp') {
      const draft = grid.draft
      if (!draft) return
      const target = msAt(x)
      let best = MIN_RAMP
      let bestGap = Infinity
      let last = MIN_RAMP
      for (let i = draft.starts[j]; i < draft.times.length && draft.sections[i] === j; i++) {
        last = draft.steps[i]
        const gap = Math.abs(draft.times[i] - target)
        if (gap < bestGap) {
          bestGap = gap
          best = draft.steps[i]
        }
      }
      const beats = e.shiftKey ? best : Math.round(best / 4) * 4
      grid.setRamp(j, Math.max(MIN_RAMP, Math.min(Math.max(MIN_RAMP, last), beats)), base)
      return
    }
    if (j === 0) {
      grid.shiftSection(0, ((e.clientX - drag.x) / pxPerSecond()) * 1000, base)
      return
    }
    let target = msAt(x)
    if (!e.altKey) {
      // Snap to where the previous section's beats fall, so the change stays on the grid.
      const draft = grid.draft
      if (draft) {
        const beats = continuedBeats(draft, base, j - 1, target + 2000)
        target = beats.reduce((a, b) => (Math.abs(b - target) < Math.abs(a - target) ? b : a), beats[0])
      }
    }
    grid.moveFlag(j, target, base)
  }

  function endDrag(): void {
    const ended = drag
    drag = null
    dragging = null
    if (!ended || ended.moved || ended.kind !== 'flag') return
    const id = grid.map?.flags[ended.j]?.id ?? null
    grid.select(grid.selected === id ? null : id)
  }
</script>

<div
  class="relative h-[22px] border-b border-line-soft bg-[oklch(0.14_0.007_290)]"
  bind:clientWidth={width}
  bind:clientHeight={height}
>
  <div
    class={[
      'absolute inset-0 touch-none',
      dragging
        ? dragging === 'flag'
          ? 'cursor-grabbing'
          : 'cursor-ew-resize'
        : hover?.kind === 'ramp'
          ? 'cursor-ew-resize'
          : hover
            ? 'cursor-grab'
            : 'cursor-default'
    ]}
    role="toolbar"
    tabindex="-1"
    aria-label="Tempo flags. Double-click to add a flag."
    title={dragging ? undefined : hint(hover)}
    onpointerdown={(e) => {
      if (e.button !== 0 || !grid.map) return
      const hit = hitAt(localX(e))
      if (!hit) return
      e.currentTarget.setPointerCapture(e.pointerId)
      drag = { kind: hit.kind, j: hit.j, x: e.clientX, base: grid.map, moved: false }
    }}
    {onpointermove}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onpointerleave={() => {
      if (!drag) hover = null
    }}
    ondblclick={(e) => {
      if (!hitAt(localX(e))) grid.addFlagAt(msAt(localX(e)))
    }}
  >
    <canvas {@attach paint} class="pointer-events-none absolute inset-0 size-full"></canvas>
  </div>
  <FlagPopover {xOf} {width} />
</div>
