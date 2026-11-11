/** Sizes `canvas` for `w` × `h` CSS pixels at the screen's density and returns a reset context. */
export function setupCanvas(canvas: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D {
  const dpr = window.devicePixelRatio || 1
  const pw = Math.max(1, Math.round(w * dpr))
  const ph = Math.max(1, Math.round(h * dpr))
  if (canvas.width !== pw || canvas.height !== ph) {
    canvas.width = pw
    canvas.height = ph
  }
  const ctx = canvas.getContext('2d')!
  ctx.setTransform(1, 0, 0, 1, 0, 0)
  return ctx
}

export type WaveAnchor = 'bottom' | 'center'

/** Runs `draw` in a band `heightScale` of `ph` tall, kept against `anchor`. */
export function drawAtHeightScale(
  ctx: CanvasRenderingContext2D,
  ph: number,
  heightScale: number,
  anchor: WaveAnchor,
  draw: (scaledPh: number) => void
): void {
  const scaledPh = Math.max(1, Math.round(ph * heightScale))
  const slack = ph - scaledPh
  ctx.save()
  ctx.translate(0, anchor === 'bottom' ? slack : Math.round(slack / 2))
  draw(scaledPh)
  ctx.restore()
}
