import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { formatTempoChange, formatKeyChange } = await import('../src/renderer/src/lib/format.ts')

describe('formatTempoChange', () => {
  it('shows zero at the track’s own tempo, even with float drift', () => {
    assert.equal(formatTempoChange(0), '0.00%')
    assert.equal(formatTempoChange(-0.0000001), '0.00%')
  })

  it('shows a signed percentage to hundredths', () => {
    assert.equal(formatTempoChange(2.5), '+2.50%')
    assert.equal(formatTempoChange(-0.456), '−0.46%')
  })
})

describe('formatKeyChange', () => {
  it('shows zero when the key has not moved', () => {
    assert.equal(formatKeyChange(-0.04), '0')
  })

  it('shows whole semitones without decimals', () => {
    assert.equal(formatKeyChange(2), '+2')
    assert.equal(formatKeyChange(-1.0001), '−1')
  })

  it('shows part of a semitone to one decimal place', () => {
    assert.equal(formatKeyChange(0.68), '+0.7')
    assert.equal(formatKeyChange(-1.26), '−1.3')
  })
})
