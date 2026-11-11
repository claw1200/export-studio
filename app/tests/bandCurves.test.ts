import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { softClip } = await import('../src/renderer/src/lib/waveform/bandCurves.ts')

const near = (actual: number, expected: number): void => assert.ok(Math.abs(actual - expected) < 1e-9)

describe('softClip', () => {
  const curve = softClip(1.5, 1)

  it('keeps silence and full scale where they are', () => {
    near(curve(0), 0)
    near(curve(1), 1)
  })

  it('lifts quiet levels and keeps rising without flattening out', () => {
    let previous = 0
    for (let level = 0.1; level <= 1; level += 0.1) {
      assert.ok(curve(level) > level)
      assert.ok(curve(level) > previous)
      previous = curve(level)
    }
  })
})
