import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { crossfade, faderPosition, soleAudible } = await import('../src/renderer/src/lib/crossfade.ts')

const close = (a: number, b: number): void => assert.ok(Math.abs(a - b) < 1e-9, `${a} ≉ ${b}`)

describe('crossfade', () => {
  it('plays both decks at full level in the centre', () => {
    assert.deepEqual(crossfade(0.5), [1, 1])
  })

  it('plays only A at the left and only B at the right', () => {
    const [a0, b0] = crossfade(0)
    assert.equal(a0, 1)
    close(b0, 0)
    const [a1, b1] = crossfade(1)
    close(a1, 0)
    assert.equal(b1, 1)
  })

  it('only turns down the deck the fader moves away from', () => {
    for (let x = 0; x <= 0.5; x += 0.05) assert.equal(crossfade(x)[0], 1)
    for (let x = 0.5; x <= 1; x += 0.05) assert.equal(crossfade(x)[1], 1)
    const [, quarterB] = crossfade(0.25)
    close(quarterB, Math.SQRT1_2)
  })

  it('fades steadily, never getting louder further from a deck', () => {
    let last = 1
    for (let x = 0.5; x <= 1.0001; x += 0.01) {
      const [a] = crossfade(x)
      assert.ok(a <= last + 1e-12)
      last = a
    }
  })

  it('clamps positions outside the travel', () => {
    assert.deepEqual(crossfade(-1), crossfade(0))
    assert.deepEqual(crossfade(2), crossfade(1))
  })
})

describe('crossfade curve', () => {
  it('defaults to full level to the centre', () => {
    for (let x = 0; x <= 1; x += 0.05) assert.deepEqual(crossfade(x, 0.5), crossfade(x))
  })

  it('smooth keeps constant power with both decks at −3 dB in the centre', () => {
    const [a, b] = crossfade(0.5, 0)
    close(a, Math.SQRT1_2)
    close(b, Math.SQRT1_2)
    for (let x = 0; x <= 1; x += 0.05) {
      const [l, r] = crossfade(x, 0)
      close(l * l + r * r, 1)
    }
  })

  it('sharp holds both decks full until near the far end, then cuts', () => {
    assert.deepEqual(crossfade(0.9, 0.95), [1, 1])
    const [a] = crossfade(1, 0.95)
    close(a, 0)
    const [mid] = crossfade(0.975, 0.95)
    close(mid, Math.SQRT1_2)
  })

  it('clamps the curve so the sharpest still fades', () => {
    assert.deepEqual(crossfade(0.97, 1), crossfade(0.97, 0.95))
    assert.deepEqual(crossfade(0.3, -1), crossfade(0.3, 0))
  })
})

describe('faderPosition', () => {
  it('snaps onto the centre when close to it', () => {
    assert.equal(faderPosition(0.51), 0.5)
    assert.equal(faderPosition(0.49), 0.5)
    assert.equal(faderPosition(0.6), 0.6)
  })

  it('stays within the travel', () => {
    assert.equal(faderPosition(-0.2), 0)
    assert.equal(faderPosition(1.4), 1)
  })
})

describe('soleAudible', () => {
  it('picks the only deck playing', () => {
    assert.equal(soleAudible([true, false], [1, 1]), 0)
    assert.equal(soleAudible([false, true], [1, 1]), 1)
  })

  it('ignores a playing deck faded out', () => {
    assert.equal(soleAudible([true, true], [1, 0]), 0)
    assert.equal(soleAudible([true, true], [0.001, 1]), 1)
  })

  it('picks nothing while both or neither are heard', () => {
    assert.equal(soleAudible([true, true], [1, 1]), null)
    assert.equal(soleAudible([false, false], [1, 1]), null)
    assert.equal(soleAudible([true, false], [0, 1]), null)
  })
})
