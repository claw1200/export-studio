import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { membership } from '../src/renderer/src/lib/library/membership.ts'

describe('membership', () => {
  const contents = new Set(['a', 'b', 'c'])

  it('reports none when no track is in the playlist', () => {
    assert.equal(membership(contents, ['x', 'y']), 'none')
  })

  it('reports all when every track is in the playlist', () => {
    assert.equal(membership(contents, ['a']), 'all')
    assert.equal(membership(contents, ['a', 'c']), 'all')
  })

  it('reports some when only part of the selection is in the playlist', () => {
    assert.equal(membership(contents, ['a', 'x']), 'some')
  })

  it('treats an empty playlist as holding none of the tracks', () => {
    assert.equal(membership(new Set(), ['a']), 'none')
  })
})
