import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { barsBetween, cueSections, formatBars } = await import('../src/renderer/src/lib/deck/sections.ts')
type Beatgrid = import('../src/renderer/src/lib/types.ts').Beatgrid
type Cue = import('../src/renderer/src/lib/types.ts').Cue

/** A steady grid at `bpm` from `firstMs`, `count` beats long. */
function grid(bpm: number, count: number, firstMs = 0): Beatgrid {
  const step = 60_000 / bpm
  const times = Array.from({ length: count }, (_, i) => firstMs + i * step)
  return { times, beats: times.map((_, i) => (i % 4) + 1), bpms: times.map(() => bpm) }
}

const cue = (seconds: number, kind: 'hot' | 'memory' = 'memory', slot = 0): Cue => ({
  id: `${kind}-${seconds}-${slot}`,
  kind,
  slot,
  start: seconds * 1000,
  end: null,
  color: null,
  colorIndex: null,
  comment: ''
})

describe('barsBetween', () => {
  it('counts bars along the beatgrid', () => {
    // 120 BPM: a beat every half second, a bar every two seconds.
    assert.equal(barsBetween(0, 32, grid(120, 200), 0), 16)
  })

  it('follows a tempo change in the grid', () => {
    const slow = grid(120, 33)
    const lastMs = slow.times[32]
    const fast = grid(240, 64, lastMs + 250)
    const changing = {
      times: [...slow.times, ...fast.times],
      beats: [...slow.beats, ...fast.beats],
      bpms: [...slow.bpms, ...fast.bpms]
    }
    // 8 bars at 120 then 8 bars at 240 (half the time).
    assert.equal(barsBetween(0, lastMs / 1000 + 8, changing, 0), 16)
  })

  it('falls back to the track tempo without a grid', () => {
    assert.equal(barsBetween(10, 26, null, 120), 8)
  })

  it('has nothing to count by without a grid or tempo', () => {
    assert.equal(barsBetween(0, 10, null, 0), null)
  })
})

describe('cueSections', () => {
  it('splits the track at every cue, in time order', () => {
    const sections = cueSections([cue(64), cue(0), cue(32)], grid(120, 200), 0)
    assert.deepEqual(
      sections.map((s) => [s.start, s.end, s.bars]),
      [
        [0, 32, 16],
        [32, 64, 16]
      ]
    )
  })

  it('treats a hot cue on a memory cue as one place', () => {
    const sections = cueSections([cue(0), cue(16), cue(16.01, 'hot', 1)], grid(120, 200), 0)
    assert.equal(sections.length, 1)
    assert.equal(sections[0].bars, 8)
  })

  it('ends with an out section from the last cue to the end of the track', () => {
    const sections = cueSections([cue(0), cue(32)], grid(120, 200), 0, 50)
    assert.equal(sections.length, 2)
    assert.deepEqual(sections[1], { start: 32, end: 50, bars: 9, edge: 'out' })
    assert.equal(sections[0].edge, undefined)
  })

  it('opens with an in section when the first cue is after the start', () => {
    const sections = cueSections([cue(8), cue(40)], grid(120, 200), 0)
    assert.deepEqual(sections[0], { start: 0, end: 8, bars: 4, edge: 'in' })
    assert.equal(sections[1].edge, undefined)
  })

  it('has no in section when the first cue is at the start', () => {
    const sections = cueSections([cue(0.02), cue(32)], grid(120, 200), 0)
    assert.equal(sections.length, 1)
    assert.equal(sections[0].edge, undefined)
  })

  it('gives a single cue an in and an out section', () => {
    assert.deepEqual(
      cueSections([cue(10)], null, 120, 18).map((s) => [s.start, s.end, s.edge]),
      [
        [0, 10, 'in'],
        [10, 18, 'out']
      ]
    )
  })

  it('leaves out the out section when the last cue is at the end', () => {
    assert.equal(cueSections([cue(0), cue(32)], grid(120, 200), 0, 32.01).length, 1)
  })

  it('has no sections without cues', () => {
    assert.deepEqual(cueSections([], null, 120, 200), [])
  })
})

describe('formatBars', () => {
  it('shows whole bars plainly', () => {
    assert.equal(formatBars(16), '16')
    assert.equal(formatBars(15.98), '16')
  })

  it('rounds anything else to the nearest quarter bar', () => {
    assert.equal(formatBars(7.5), '7.5')
    assert.equal(formatBars(3.26), '3.25')
  })
})
