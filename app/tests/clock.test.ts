import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { follow, SNAP_SECONDS } = await import('../src/renderer/src/lib/deck/clock.ts')

const FRAME_MS = 16
const FRAME_S = FRAME_MS / 1000
const SMALL_DRIFT = 0.01
const ENGINE_JITTER = 0.003
/** Close enough to a loop edge that the engine and the drawn playhead disagree on which side. */
const A_MOMENT = 0.001
/** An eighth-beat loop at 128 BPM: shorter than the snap threshold. */
const SHORT_LOOP = [10, 10 + 60 / 128 / 8] as const

function distanceRoundLoop(a: number, b: number): number {
  const length = SHORT_LOOP[1] - SHORT_LOOP[0]
  const straight = Math.abs(a - b)
  return Math.min(straight, length - straight)
}

describe('follow', () => {
  it('eases a small drift', () => {
    const next = follow(5, 5 + FRAME_S + SMALL_DRIFT, FRAME_MS, FRAME_S)
    assert.ok(next > 5 + FRAME_S && next < 5 + FRAME_S + SMALL_DRIFT)
  })

  it('snaps to a large jump', () => {
    assert.equal(follow(5, 9, FRAME_MS, FRAME_S), 9)
  })

  it('wraps with the engine in a short loop', () => {
    const [start, end] = SHORT_LOOP
    const shown = end - FRAME_S / 2
    const target = start + FRAME_S / 2
    const next = follow(shown, target, FRAME_MS, FRAME_S, SHORT_LOOP)
    assert.ok(Math.abs(next - target) < FRAME_S / 2, `expected near ${target}, got ${next}`)
  })

  it('does not glide back when the engine wraps a frame early', () => {
    const [start, end] = SHORT_LOOP
    assert.ok(end - start < SNAP_SECONDS)
    const target = start + A_MOMENT
    const next = follow(end - 2 * FRAME_S, target, FRAME_MS, FRAME_S, SHORT_LOOP)
    assert.ok(distanceRoundLoop(next, target) < FRAME_S, `glided to ${next}`)
  })

  it('does not glide forward when the engine wraps a frame late', () => {
    const target = SHORT_LOOP[1] - A_MOMENT
    const next = follow(SHORT_LOOP[1] - FRAME_S / 2, target, FRAME_MS, FRAME_S, SHORT_LOOP)
    assert.ok(distanceRoundLoop(next, target) < FRAME_S, `glided to ${next}`)
  })

  it('stays inside the loop over many frames', () => {
    const [start, end] = SHORT_LOOP
    const length = end - start
    let shown = start
    for (let frame = 1; frame <= 200; frame++) {
      const engine = start + ((frame * FRAME_S + ENGINE_JITTER * Math.sin(frame)) % length)
      shown = follow(shown, engine, FRAME_MS, FRAME_S, SHORT_LOOP)
      assert.ok(shown >= start && shown < end, `left the loop at frame ${frame}: ${shown}`)
      assert.ok(distanceRoundLoop(shown, engine) < FRAME_S, `drifted from ${engine} to ${shown} at frame ${frame}`)
    }
  })
})
