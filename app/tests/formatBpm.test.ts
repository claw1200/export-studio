import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { formatBpm } = await import('../src/renderer/src/lib/format.ts')

describe('formatBpm', () => {
  it('shows whole BPMs without decimals', () => {
    assert.equal(formatBpm(128), '128')
  })

  it('does not show float drift from a tempo change as decimals', () => {
    assert.equal(formatBpm(130.00000000001), '130')
    assert.equal(formatBpm(129.99999999), '130')
  })

  it('shows fractional BPMs to hundredths', () => {
    assert.equal(formatBpm(128.5), '128.50')
    assert.equal(formatBpm(127.456), '127.46')
  })

  it('is empty without a BPM', () => {
    assert.equal(formatBpm(0), '')
  })
})
