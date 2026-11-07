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
    const late = { start: 0.5, end: 1 }
    // To 0.8 is 60% swapped; turning back holds it there until the fader has come back half way.
    assert.deepEqual(sweep(0, [0.8, 0.6, 0.5, 0.3, 0.1, 0], late), [0.6, 0.6, 0.6, 0.6, 0.2, 0])
  })

  it('ends clean at either side after a wiggle', () => {
    const late = { start: 0.5, end: 1 }
    assert.deepEqual(sweep(0, [0.8, 0.5, 1], late), [0.6, 0.6, 1])
    assert.deepEqual(sweep(1, [0.2, 0.5, 0], late), [0.4, 0.4, 0])
  })

  it('never jumps and always ends clean, however the fader is wiggled', () => {
    const ranges = [
      { start: 0, end: 0.5 },
      { start: 0.25, end: 0.75 },
      { start: 0.5, end: 1 }
    ]
    let seed = 7
    const random = (): number => (seed = (seed * 16807) % 2147483647) / 2147483647
    for (const range of ranges) {
      const rate = 1 / (range.end - range.start)
      for (let run = 0; run < 200; run++) {
        let swap = startBassSwap(random(), range)
        for (let step = 0; step < 60; step++) {
          const position = Math.min(1, Math.max(0, swap.position + (random() - 0.5) * 0.2))
          const next = bassSwap(swap, position, range)
          const change = Math.abs(next.share - swap.share)
          assert.ok(change <= Math.abs(position - swap.position) * rate + 1e-9, `jumped by ${change}`)
          swap = next
        }
        assert.equal(bassSwap(swap, 1, range).share, 1)
        assert.equal(bassSwap(swap, 0, range).share, 0)
      }
    }
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
