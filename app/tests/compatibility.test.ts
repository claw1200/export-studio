import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { assess } = await import('../src/renderer/src/lib/hardware/compatibility.ts')
const { DEFAULT_GREEN } = await import('../src/renderer/src/lib/hardware/players.ts')
const { introduceViews } = await import('../src/renderer/src/lib/library/defaults.ts')
const { keepViews } = await import('../src/renderer/src/lib/library/views.ts')
import type { AudioFacts } from '../src/renderer/src/lib/hardware/compatibility.ts'
import type { View } from '../src/renderer/src/lib/types.ts'

const targets = { greenId: DEFAULT_GREEN }

function facts(row: Partial<AudioFacts> = {}): AudioFacts {
  return {
    kind: 'WAV',
    sampleRate: 44100,
    bitDepth: 16,
    channels: 2,
    bitrate: 0,
    encoding: 'pcm',
    ...row
  }
}

describe('assess', () => {
  it('marks a 44.1 kHz 16-bit wav green on the defaults', () => {
    const verdict = assess(facts(), targets)
    assert.equal(verdict.color, 'green')
    assert.match(verdict.summary, /XDJ-1000MK2/)
    assert.deepEqual(verdict.problems, ['WAV file'])
    assert.deepEqual(verdict.incompatible, ['CDJ-400'])
  })

  it('keeps 48 kHz flac green and names every player that cannot load it', () => {
    const verdict = assess(facts({ kind: 'FLAC', encoding: 'flac', sampleRate: 48000 }), targets)
    assert.equal(verdict.color, 'green')
    assert.deepEqual(verdict.problems, ['FLAC file'])
    assert.ok(verdict.incompatible.includes('CDJ-2000'))
    assert.ok(verdict.incompatible.includes('CDJ-900NXS'))
    assert.ok(verdict.incompatible.includes('XDJ-700'))
    assert.ok(verdict.incompatible.includes('XDJ-1000'))
    assert.equal(verdict.incompatible.includes('XDJ-1000MK2'), false)
  })

  it('marks 96 kHz flac yellow with the file problems, then one incompatible list', () => {
    const verdict = assess(facts({ kind: 'FLAC', encoding: 'flac', sampleRate: 96000 }), targets)
    assert.equal(verdict.color, 'yellow')
    assert.equal(verdict.summary, '')
    assert.deepEqual(verdict.problems, ['FLAC file', 'FLAC file is 96 kHz'])
    assert.ok(verdict.incompatible.includes('XDJ-1000MK2'))
    assert.ok(verdict.incompatible.includes('CDJ-1500X'))
    assert.ok(verdict.incompatible.includes('XDJ-700'))
  })

  it('marks a missing file red', () => {
    const verdict = assess(facts(), { ...targets, missing: true })
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, 'File missing')
    assert.deepEqual(verdict.problems, [])
    assert.deepEqual(verdict.incompatible, [])
  })

  it('marks 32-bit float yellow', () => {
    const verdict = assess(facts({ bitDepth: 32, encoding: 'float' }), targets)
    assert.equal(verdict.color, 'yellow')
    assert.deepEqual(verdict.problems, ['32-bit float'])
  })
})

describe('damaged tracks removal', () => {
  const duplicate: View = {
    id: 'default:duplicates',
    name: 'Suspected duplicates',
    type: 'duplicates',
    match: { title: true, artist: true, duration: true, fileSize: false },
    durationSlack: 2
  }

  it('does not seed Damaged tracks', () => {
    const seeded = introduceViews([], '', false, [])
    assert.equal(
      seeded.views.some((view) => view.id === 'default:problems'),
      false
    )
    assert.ok(seeded.views.some((view) => view.id === 'default:duplicates'))
    assert.equal(seeded.introduced.includes('default:problems'), false)
  })

  it('does not bring a deleted built-in back, and does not add a problems view later', () => {
    const seeded = introduceViews([duplicate], '', true, [])
    assert.equal(
      seeded.views.some((view) => view.id === 'default:problems'),
      false
    )
    assert.equal(seeded.views.filter((view) => view.id === 'default:duplicates').length, 1)
    const again = introduceViews(seeded.views, '', true, seeded.introduced)
    assert.equal(
      again.views.some((view) => view.id === 'default:problems'),
      false
    )
  })

  it('drops a stored problems view on load', () => {
    const kept = keepViews([
      { id: 'default:problems', name: 'Damaged tracks', type: 'problems', checks: { missing: true } },
      duplicate
    ])
    assert.deepEqual(
      kept.map((view) => view.id),
      ['default:duplicates']
    )
  })
})
