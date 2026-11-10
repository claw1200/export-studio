import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { followCurve } = await import('../src/renderer/src/lib/waveform/bandCurves.ts')

const curve = [
  [0, 0],
  [0.5, 0.8],
  [1, 1]
] as const

describe('followCurve', () => {
  it('lands on each point', () => {
    assert.equal(followCurve(curve, 0), 0)
    assert.equal(followCurve(curve, 0.5), 0.8)
    assert.equal(followCurve(curve, 1), 1)
  })

  it('draws a straight line between points', () => {
    assert.equal(followCurve(curve, 0.25), 0.4)
    assert.ok(Math.abs(followCurve(curve, 0.75) - 0.9) < 1e-9)
  })
})
