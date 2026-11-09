import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { stepTempo } = await import('../src/renderer/src/lib/mix/sync.ts')

const close = (a: number, b: number, eps = 1e-9): void => assert.ok(Math.abs(a - b) < eps, `${a} ≉ ${b}`)

describe('stepTempo', () => {
  it('moves the playing tempo by a whole BPM', () => {
    close(stepTempo(1, 128, 1) ?? 0, 129 / 128)
    close(stepTempo(130 / 128, 128, -1) ?? 0, 129 / 128)
  })

  it('stays within the engine’s speed range', () => {
    assert.equal(stepTempo(255.5 / 128, 128, 1), 2)
    assert.equal(stepTempo(64.5 / 128, 128, -1), 0.5)
  })

  it('needs a tempo to step from', () => {
    assert.equal(stepTempo(1, 0, 1), null)
  })
})
