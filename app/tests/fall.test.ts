import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { bandFall } = await import('../src/renderer/src/lib/waveform/fall.ts')

function lowBand(levels: number[]): Uint8Array {
  return new Uint8Array(levels.flatMap((level) => [level, 0, 0]))
}

function lowLevels(bands: Uint8Array): number[] {
  return Array.from({ length: bands.length / 3 }, (_, i) => bands[i * 3])
}

describe('bandFall', () => {
  it('rises at once and eases down by a constant ratio per block', () => {
    const fallen = lowLevels(bandFall(lowBand([0, 200, 0, 0, 0]), 1, 2))
    const ratio = Math.exp(-1 / 2)
    assert.deepEqual(fallen, [
      0,
      200,
      Math.round(200 * ratio),
      Math.round(200 * ratio ** 2),
      Math.round(200 * ratio ** 3)
    ])
  })

  it('lets a louder entry through during a fall', () => {
    const fallen = lowLevels(bandFall(lowBand([200, 0, 250]), 1, 10))
    assert.equal(fallen[2], 250)
  })

  it('reads the peak of each stride before falling', () => {
    const fallen = lowLevels(bandFall(lowBand([10, 90, 0, 0]), 2, 1))
    assert.deepEqual(fallen, [90, Math.round(90 * Math.exp(-1))])
  })
})
