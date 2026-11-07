import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { keyMatch, toCamelot, withinCutoff, keyMatchColor, heardKey } =
  await import('../src/renderer/src/lib/mix/harmonic.ts')

describe('keyMatch', () => {
  it('ranks moves round the wheel from 1A', () => {
    assert.equal(keyMatch('1A', '1A'), 'same')
    assert.equal(keyMatch('1A', '1B'), 'relative')
    assert.equal(keyMatch('1A', '2A'), 'adjacent')
    assert.equal(keyMatch('1A', '12A'), 'adjacent')
    assert.equal(keyMatch('1A', '3A'), 'two')
    assert.equal(keyMatch('1A', '8A'), 'boost')
  })

  it('leaves other moves unmatched', () => {
    assert.equal(keyMatch('1A', '6A'), null)
    assert.equal(keyMatch('3A', '1A'), null)
    assert.equal(keyMatch('1A', '11A'), null)
    assert.equal(keyMatch('1A', '4A'), null)
    assert.equal(keyMatch('1A', '2B'), null)
    assert.equal(keyMatch('1A', ''), null)
    assert.equal(keyMatch('', '1A'), null)
  })

  it('works for major keys and across 12 to 1', () => {
    assert.equal(keyMatch('12B', '1B'), 'adjacent')
    assert.equal(keyMatch('11B', '1B'), 'two')
    assert.equal(keyMatch('8B', '8A'), 'relative')
    assert.equal(keyMatch('6B', '1B'), 'boost')
  })

  it('compares note names with Camelot', () => {
    assert.equal(keyMatch('Am', '8A'), 'same')
    assert.equal(keyMatch('8A', 'C'), 'relative')
    assert.equal(keyMatch('Am', 'Em'), 'adjacent')
  })
})

describe('toCamelot', () => {
  it('places note names on the wheel', () => {
    assert.deepEqual(toCamelot('C'), { n: 8, minor: false })
    assert.deepEqual(toCamelot('Am'), { n: 8, minor: true })
    assert.deepEqual(toCamelot('G#m'), { n: 1, minor: true })
    assert.deepEqual(toCamelot('Abm'), { n: 1, minor: true })
    assert.deepEqual(toCamelot('F'), { n: 7, minor: false })
    assert.deepEqual(toCamelot('Gm'), { n: 6, minor: true })
  })

  it('rejects what is not a key', () => {
    assert.equal(toCamelot('13A'), null)
    assert.equal(toCamelot('0B'), null)
    assert.equal(toCamelot('H'), null)
  })
})

describe('withinCutoff', () => {
  it('keeps matches no looser than the cutoff', () => {
    assert.equal(withinCutoff('same', 'same'), true)
    assert.equal(withinCutoff('relative', 'same'), false)
    assert.equal(withinCutoff('two', 'boost'), true)
    assert.equal(withinCutoff('boost', 'two'), false)
  })

  it('highlights nothing when off or unmatched', () => {
    assert.equal(withinCutoff('same', null), false)
    assert.equal(withinCutoff(null, 'boost'), false)
  })
})

describe('keyMatchColor', () => {
  it('runs from green to orange', () => {
    assert.match(keyMatchColor('same'), / 150\)$/)
    assert.match(keyMatchColor('boost'), / 55\)$/)
  })

  it('keeps a one-step match green', () => {
    const hue = (color: string): number => Number(/ (\d+)\)$/.exec(color)![1])
    assert.ok(hue(keyMatchColor('adjacent')) >= 120)
    assert.ok(hue(keyMatchColor('adjacent')) > hue(keyMatchColor('two')))
  })
})

describe('heardKey', () => {
  it('follows the key shift and the pitch of the tempo', () => {
    assert.equal(heardKey('1A', { rate: 1, keyShift: 1, masterTempo: false }), '8A')
    assert.equal(heardKey('1A', { rate: 2 ** (1 / 12), keyShift: 0, masterTempo: false }), '8A')
    assert.equal(heardKey('1A', { rate: 2 ** (1 / 12), keyShift: 0, masterTempo: true }), '1A')
    assert.equal(heardKey('', { rate: 1, keyShift: 3, masterTempo: false }), '')
  })
})
