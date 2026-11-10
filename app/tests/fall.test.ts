import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { bandFall, slidingMax } = await import('../src/renderer/src/lib/waveform/fall.ts')

function lowBand(levels: number[]): Uint8Array {
  return new Uint8Array(levels.flatMap((level) => [level, 0, 0]))
}

function lowLevels(bands: Uint8Array): number[] {
  return Array.from({ length: bands.length / 3 }, (_, i) => bands[i * 3])
}

function decayOnly(timeConstantBlocks: number): { blocksPerSecond: number; timeConstantBlocks: number } {
  return { blocksPerSecond: 1, timeConstantBlocks }
}

describe('bandFall', () => {
  it('rises at once and eases down by a constant ratio per block', () => {
    const fallen = lowLevels(bandFall(lowBand([0, 200, 0, 0, 0]), 1, decayOnly(2)))
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
    const fallen = lowLevels(bandFall(lowBand([200, 0, 250]), 1, decayOnly(10)))
    assert.equal(fallen[2], 250)
  })

  it('reads the peak of each stride before falling', () => {
    const fallen = lowLevels(bandFall(lowBand([10, 90, 0, 0]), 2, decayOnly(1)))
    assert.deepEqual(fallen, [90, Math.round(90 * Math.exp(-1))])
  })

  it('holds a bass level across the dips of its rectified cycle', () => {
    const lowestBassHz = 40
    const quarterCycleBlocks = 2
    const blocksPerSecond = lowestBassHz * 4 * quarterCycleBlocks
    const rectifiedBass = [200, 120, 0, 120, 200, 120, 0, 120, 200]
    const fallen = lowLevels(bandFall(lowBand(rectifiedBass), 1, { blocksPerSecond, timeConstantBlocks: 1 }))
    assert.deepEqual(fallen, Array(rectifiedBass.length).fill(200))
  })
})

describe('slidingMax', () => {
  it('matches a brute-force scan of every window', () => {
    const column = Uint8Array.from({ length: 101 }, (_, i) => (i * 73 + 11) % 256)
    for (const reach of [1, 2, 5, 13]) {
      const expected = Array.from(column, (_, i) =>
        Math.max(...column.subarray(Math.max(0, i - reach), Math.min(column.length, i + reach + 1)))
      )
      assert.deepEqual(Array.from(slidingMax(column, reach)), expected)
    }
  })
})
