import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { cueToStore } from '../src/renderer/src/lib/deck/transport.ts'

describe('cueToStore', () => {
  it('stores a plain cue without a loop', () => {
    assert.deepEqual(cueToStore(12.5, null), { start: 12.5, end: null })
  })

  it('stores the loop while one plays, wherever the cue is set', () => {
    const loop = { start: 10, end: 14, active: true }
    assert.deepEqual(cueToStore(10, loop), { start: 10, end: 14 })
    assert.deepEqual(cueToStore(12.3, loop), { start: 10, end: 14 })
  })

  it('stores an exited loop only when set at its in point', () => {
    const loop = { start: 10, end: 14, active: false }
    assert.deepEqual(cueToStore(10.0004, loop), { start: 10, end: 14 })
    assert.deepEqual(cueToStore(12.3, loop), { start: 12.3, end: null })
  })

  it('ignores a loop with no length', () => {
    assert.deepEqual(cueToStore(10, { start: 10, end: 10, active: true }), { start: 10, end: null })
  })
})
