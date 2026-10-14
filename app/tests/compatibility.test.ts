import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { assess, verdictForTrack } = await import('../src/renderer/src/lib/hardware/compatibility.ts')
const { DEFAULT_GREEN, PLAYERS } = await import('../src/renderer/src/lib/hardware/players.ts')
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

describe('verdictForTrack', () => {
  const track = {
    fileType: 'WAV',
    sampleRate: 44100,
    bitDepth: 16,
    bitrate: 0,
    missing: false,
    streaming: false
  }

  it('uses the library row when no header has been read', () => {
    assert.equal(verdictForTrack(track, undefined, DEFAULT_GREEN).color, 'green')
  })

  it('prefers a header probe over the library row', () => {
    const verdict = verdictForTrack(
      track,
      {
        missing: false,
        unreadable: false,
        audio: { kind: 'FLAC', sampleRate: 48000, bitDepth: 24, channels: 2, bitrate: 0, encoding: 'flac' }
      },
      DEFAULT_GREEN
    )
    assert.equal(verdict.color, 'yellow')
  })

  it('treats a missing file as red even when the row looks playable', () => {
    const verdict = verdictForTrack({ ...track, missing: true }, undefined, DEFAULT_GREEN)
    assert.equal(verdict.summary, 'File missing')
  })
})

describe('assess', () => {
  it('marks a 44.1 kHz 16-bit wav green on the defaults', () => {
    const verdict = assess(facts(), targets)
    assert.equal(verdict.color, 'green')
    assert.match(verdict.summary, /XDJ-1000MK2/)
    assert.deepEqual(verdict.problems, ['WAV file'])
    assert.deepEqual(verdict.incompatible, ['CDJ-400'])
  })

  it('marks 48 kHz flac yellow when a newer player cannot load it', () => {
    const verdict = assess(facts({ kind: 'FLAC', encoding: 'flac', sampleRate: 48000 }), targets)
    assert.equal(verdict.color, 'yellow')
    assert.equal(verdict.summary, 'Plays on XDJ-1000MK2')
    assert.deepEqual(verdict.problems, ['FLAC file'])
    for (const name of ['CDJ-2000', 'CDJ-900NXS', 'XDJ-700', 'XDJ-1000', 'XDJ-RX2', 'XDJ-RR', 'MEP-7000']) {
      assert.equal(verdict.incompatible.includes(name), true, name)
    }
    for (const name of ['XDJ-1000MK2', 'XDJ-RX3', 'OMNIS-DUO', 'CDJ-TOUR1']) {
      assert.equal(verdict.incompatible.includes(name), false, name)
    }
  })

  it('marks 96 kHz flac red when the selected player stops at 48 kHz', () => {
    const verdict = assess(facts({ kind: 'FLAC', encoding: 'flac', sampleRate: 96000 }), targets)
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, "Won't play on XDJ-1000MK2")
    assert.deepEqual(verdict.problems, ['FLAC file', 'FLAC file is 96 kHz'])
    assert.ok(verdict.incompatible.includes('XDJ-1000MK2'))
    assert.ok(verdict.incompatible.includes('CDJ-1500X'))
    assert.ok(verdict.incompatible.includes('OMNIS-DUO'))
    assert.ok(verdict.incompatible.includes('XDJ-RX3'))
    assert.equal(verdict.incompatible.includes('CDJ-TOUR1'), false)
  })

  it('marks alac yellow when a newer player cannot load it', () => {
    const verdict = assess(facts({ kind: 'ALAC', encoding: 'alac' }), targets)
    assert.equal(verdict.color, 'yellow')
    assert.equal(verdict.summary, 'Plays on XDJ-1000MK2')
    assert.deepEqual(verdict.problems, ['ALAC file'])
    assert.ok(verdict.incompatible.includes('XDJ-XZ'))
    assert.ok(verdict.incompatible.includes('XDJ-RX3'))
    assert.equal(verdict.incompatible.includes('XDJ-1000MK2'), false)
    assert.equal(verdict.incompatible.includes('OMNIS-DUO'), false)
  })

  it('marks a missing file red', () => {
    const verdict = assess(facts(), { ...targets, missing: true })
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, 'File missing')
    assert.deepEqual(verdict.problems, [])
    assert.deepEqual(verdict.incompatible, [])
  })

  it('lists every rekordbox usb player', () => {
    const names = new Set(PLAYERS.map((player) => player.name))
    for (const name of [
      'XDJ-AERO',
      'XDJ-R1',
      'XDJ-RX',
      'XDJ-RX2',
      'XDJ-RR',
      'XDJ-RX3',
      'XDJ-XZ',
      'XDJ-AZ',
      'OPUS-QUAD',
      'OMNIS-DUO',
      'CDJ-3000X',
      'CDJ-3000',
      'CDJ-2000NXS2',
      'CDJ-2000NXS',
      'CDJ-2000',
      'CDJ-900NXS',
      'CDJ-900',
      'CDJ-850',
      'CDJ-350',
      'CDJ-400',
      'CDJ-TOUR1',
      'XDJ-1000MK2',
      'XDJ-1000',
      'XDJ-700',
      'MEP-7000'
    ]) {
      assert.equal(names.has(name), true, name)
    }
  })

  it('marks aac red on the CDJ-400, which plays MP3 only', () => {
    const verdict = assess(facts({ kind: 'AAC', encoding: 'aac', bitDepth: 16, bitrate: 256 }), {
      greenId: 'cdj-400'
    })
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, "Won't play on CDJ-400")
    assert.deepEqual(verdict.problems, ['AAC file'])
  })

  it('marks a 48 kHz wav red on the MEP-7000', () => {
    const verdict = assess(facts({ sampleRate: 48000 }), { greenId: 'mep-7000' })
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, "Won't play on MEP-7000")
    assert.ok(verdict.problems.includes('WAV file is 48 kHz'))
  })

  it('marks 32-bit float red because the selected player cannot load it', () => {
    const verdict = assess(facts({ bitDepth: 32, encoding: 'float' }), targets)
    assert.equal(verdict.color, 'red')
    assert.equal(verdict.summary, "Won't play on XDJ-1000MK2")
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
