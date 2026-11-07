import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { bassLevel, bassSwap, rebaseBassSwap, startBassSwap, swapRange } =
  await import('../src/renderer/src/lib/crossfade.ts')

/** Moves the fader through `positions` in turn and returns B's share of the bass at each. */
function sweep(from: number, positions: number[], range?: { start: number; end: number }): number[] {
  let swap = startBassSwap(from)
  return positions.map((position) => {
    swap = bassSwap(swap, position, range)
    return Math.round(swap.share * 1e9) / 1e9
  })
}

describe('bass swap', () => {
  it('swaps over the first half of a fade from A to B, then holds', () => {
    assert.deepEqual(sweep(0, [0, 0.25, 0.5, 0.75, 1]), [0, 0.5, 1, 1, 1])
  })

  it('swaps back over the first half of a fade from B to A', () => {
    assert.deepEqual(sweep(1, [1, 0.75, 0.5, 0.25, 0]), [1, 0.5, 0, 0, 0])
  })

  it('swaps back from wherever the fader turns round, without a jump', () => {
    // Past the swap to 0.6, then back: the bass returns to A over the next half of travel.
    assert.deepEqual(sweep(0, [0.3, 0.6, 0.5, 0.35, 0.1, 0]), [0.6, 1, 0.8, 0.5, 0, 0])
  })

  it('a jump across the fader lands with the swap complete', () => {
    assert.deepEqual(sweep(0, [1]), [1])
    assert.deepEqual(sweep(1, [0]), [0])
  })

  it('starts with the bass on the nearer deck', () => {
    assert.equal(startBassSwap(0.3).share, 0)
    assert.equal(startBassSwap(0.8).share, 1)
  })

  it('turning back part way swaps back at the same rate', () => {
    assert.deepEqual(sweep(0, [0.3, 0.2, 0.05]), [0.6, 0.4, 0.1])
  })

  it('swaps only between the start and end of a set range', () => {
    const range = { start: 0.25, end: 0.75 }
    assert.deepEqual(sweep(0, [0.1, 0.25, 0.5, 0.75, 1], range), [0, 0, 0.5, 1, 1])
    assert.deepEqual(sweep(1, [0.9, 0.75, 0.5, 0.25, 0], range), [1, 1, 0.5, 0, 0])
  })

  it('waits out the start again after turning round, without a jump', () => {
    const range = { start: 0.1, end: 0.5 }
    // A to 0.3 is half the swap; back 0.1 is inside the start, then the half swaps back.
    assert.deepEqual(sweep(0, [0.3, 0.2, 0.1, 0], range), [0.5, 0.5, 0.25, 0])
  })

  it('carries on from where it stands when the range changes', () => {
    let swap = bassSwap(startBassSwap(0), 0.25)
    assert.equal(swap.share, 0.5)
    swap = rebaseBassSwap(swap.share, swap.position)
    swap = bassSwap(swap, 0.3, { start: 0, end: 0.2 })
    assert.ok(Math.abs(swap.share - 0.75) < 1e-9)
  })

  it('keeps the range within the travel and never instant', () => {
    assert.deepEqual(swapRange({ start: 0.5, end: 0.5 }), { start: 0.5, end: 0.55 })
    assert.deepEqual(swapRange({ start: 1, end: 1 }), { start: 0.95, end: 1 })
    assert.deepEqual(swapRange({ start: -1, end: 2 }), { start: 0, end: 1 })
  })

  it('keeps the power of one bass part way through', () => {
    assert.equal(bassLevel(0), 0)
    assert.equal(bassLevel(1), 1)
    const half = bassLevel(0.5)
    assert.ok(Math.abs(half * half * 2 - 1) < 1e-12)
  })
})
