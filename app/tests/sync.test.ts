import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { beatOffset, matchTempo, stepTempo } = await import('../src/renderer/src/lib/mix/sync.ts')

const close = (a: number, b: number, eps = 1e-9): void => assert.ok(Math.abs(a - b) < eps, `${a} ≉ ${b}`)

describe('matchTempo', () => {
  it('speeds a slower track up to the master', () => {
    const match = matchTempo(128, 124)
    assert.ok(match)
    close(match.rate, 128 / 124)
    assert.equal(match.beatsPerMasterBeat, 1)
  })

  it('leaves a track already at the master tempo alone', () => {
    assert.deepEqual(matchTempo(126, 126), { rate: 1, beatsPerMasterBeat: 1 })
  })

  it('follows at half time when that keeps the track at its own pace', () => {
    // 87 against 174: half the master is exactly the track's own tempo.
    assert.deepEqual(matchTempo(174, 87), { rate: 1, beatsPerMasterBeat: 0.5 })
  })

  it('follows at double time for a much faster master the other way', () => {
    assert.deepEqual(matchTempo(70, 140), { rate: 1, beatsPerMasterBeat: 2 })
  })

  it('picks whichever of same, half or double is nearest the file speed', () => {
    // 100 BPM following 150: 150 needs +50%, 75 needs −25%. The smaller change wins.
    const match = matchTempo(150, 100)
    assert.ok(match)
    assert.equal(match.beatsPerMasterBeat, 0.5)
    close(match.rate, 0.75)
  })

  it('has nothing to match without a tempo on both decks', () => {
    assert.equal(matchTempo(0, 120), null)
    assert.equal(matchTempo(120, 0), null)
  })
})

describe('beatOffset', () => {
  it('is zero when both decks sit on a beat', () => {
    assert.equal(beatOffset(16, 64), 0)
  })

  it('is positive when the follower is ahead of the beat', () => {
    close(beatOffset(10, 20.25), 0.25)
  })

  it('is negative when the follower is behind', () => {
    close(beatOffset(10.5, 20.25), -0.25)
  })

  it('takes the shorter way round the beat', () => {
    // 0.9 of a beat ahead is the same as 0.1 behind.
    close(beatOffset(4, 8.9), -0.1)
    close(beatOffset(4.9, 8), 0.1)
  })

  it('counts the master in the follower’s beats at half or double time', () => {
    // Following at double time: master beat 3.25 is 6.5 of the follower's, so 7.5 is on it.
    close(beatOffset(3.25, 7.5, 2), 0)
    close(beatOffset(3.25, 7.75, 2), 0.25)
  })

  it('works before the first beat of the grid', () => {
    close(beatOffset(-0.25, 0.75), 0)
  })
})

describe('stepTempo', () => {
  it('moves the playing tempo by a whole BPM', () => {
    close(stepTempo(128, 128, 1) ?? 0, 129 / 128)
    close(stepTempo(130, 128, -1) ?? 0, 129 / 128)
  })

  it('stays within the engine’s speed range', () => {
    assert.equal(stepTempo(255.5, 128, 1), 2)
    assert.equal(stepTempo(64.5, 128, -1), 0.5)
  })

  it('needs a tempo to step from', () => {
    assert.equal(stepTempo(128, 0, 1), null)
  })
})
