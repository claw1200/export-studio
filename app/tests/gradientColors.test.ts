import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { gradientColors, lowToHighBalance } = await import('../src/renderer/src/lib/waveform/gradientColors.ts')

const RED_GREEN_BLUE = ['#ff0000', '#00ff00', '#0000ff']

function entryColors(colors: Uint8Array | null): number[][] {
  assert.ok(colors)
  return Array.from({ length: colors.length / 3 }, (_, i) => Array.from(colors.subarray(i * 3, i * 3 + 3)))
}

describe('lowToHighBalance', () => {
  it('runs from all low to all high, with an even mix in the middle', () => {
    assert.equal(lowToHighBalance(200, 0, 0), 0)
    assert.equal(lowToHighBalance(0, 200, 0), 0.5)
    assert.equal(lowToHighBalance(0, 0, 200), 1)
    assert.equal(lowToHighBalance(100, 0, 100), 0.5)
  })

  it('treats silence as low', () => {
    assert.equal(lowToHighBalance(0, 0, 0), 0)
  })
})

describe('gradientColors', () => {
  it('colours each entry by where its bands sit between the stops', () => {
    const bands = new Uint8Array([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 255])
    const colors = gradientColors(bands, 1, 1, 4, RED_GREEN_BLUE)
    assert.deepEqual(entryColors(colors), [
      [255, 0, 0],
      [0, 255, 0],
      [0, 0, 255],
      [0, 255, 0]
    ])
  })

  it('fills every waveform entry, repeating the last band block past the end of the bands', () => {
    const bands = new Uint8Array([0, 0, 255])
    assert.deepEqual(entryColors(gradientColors(bands, 1, 1, 3, RED_GREEN_BLUE)), [
      [0, 0, 255],
      [0, 0, 255],
      [0, 0, 255]
    ])
  })

  it('reads the band peaks of each entry when the bands are finer than the waveform', () => {
    const bands = new Uint8Array([0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255])
    assert.deepEqual(entryColors(gradientColors(bands, 2, 1, 2, RED_GREEN_BLUE)), [
      [255, 0, 0],
      [0, 0, 255]
    ])
  })

  it('recolours when the stops change', () => {
    const bands = new Uint8Array([255, 0, 0])
    gradientColors(bands, 1, 1, 1, RED_GREEN_BLUE)
    assert.deepEqual(entryColors(gradientColors(bands, 1, 1, 1, ['#ffffff', '#00ff00', '#0000ff'])), [[255, 255, 255]])
  })
})
