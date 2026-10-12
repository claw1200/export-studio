import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { duplicateGroups, normalizeMatchText, rankCopies } from '../src/renderer/src/lib/library/duplicates.ts'
import type { DuplicateMatch, Track } from '../src/renderer/src/lib/types.ts'

function track(row: Partial<Track> & Pick<Track, 'id' | 'title'>): Track {
  return {
    artist: '',
    album: '',
    genre: '',
    label: '',
    remixer: '',
    originalArtist: '',
    composer: '',
    key: '',
    bpm: 0,
    duration: 0,
    rating: 0,
    color: 0,
    comment: '',
    year: 0,
    dateAdded: '',
    dateCreated: '',
    playCount: 0,
    bitrate: 0,
    sampleRate: 0,
    bitDepth: 0,
    fileType: '',
    storedType: null,
    path: '',
    missing: false,
    streaming: false,
    fileSize: 0,
    hasArtwork: false,
    trackNo: 0,
    analysed: false,
    myTags: [],
    cueStars: 0,
    cueCount: 0,
    phrase: false,
    ...row
  }
}

const loose: DuplicateMatch = { title: true, artist: true, duration: true, fileSize: false }

describe('normalizeMatchText', () => {
  it('folds case and punctuation and keeps mix names', () => {
    assert.equal(normalizeMatchText('  Hello, WORLD! '), 'hello world')
    assert.notEqual(normalizeMatchText('Track (Original Mix)'), normalizeMatchText('Track (Dub Mix)'))
  })
})

describe('duplicateGroups', () => {
  it('groups the same title and artist within the duration tolerance', () => {
    const groups = duplicateGroups(
      [
        track({ id: 'a', title: 'Song', artist: 'DJ', duration: 180 }),
        track({ id: 'b', title: 'song', artist: 'dj', duration: 182 }),
        track({ id: 'c', title: 'Song', artist: 'DJ', duration: 190 }),
        track({ id: 'd', title: 'Other', artist: 'DJ', duration: 180 })
      ],
      loose,
      2
    )
    assert.deepEqual(
      groups.map((group) => group.map((item) => item.id)),
      [['a', 'b']]
    )
  })

  it('leaves a blank title out when title matching is on', () => {
    const groups = duplicateGroups(
      [
        track({ id: 'a', title: '  ', artist: 'DJ', duration: 180 }),
        track({ id: 'b', title: '', artist: 'DJ', duration: 180 })
      ],
      loose,
      2
    )
    assert.deepEqual(groups, [])
  })

  it('splits on file size and skips an unknown size', () => {
    const match: DuplicateMatch = { title: true, artist: false, duration: false, fileSize: true }
    const groups = duplicateGroups(
      [
        track({ id: 'a', title: 'Song', fileSize: 10 }),
        track({ id: 'b', title: 'Song', fileSize: 10 }),
        track({ id: 'c', title: 'Song', fileSize: 11 }),
        track({ id: 'd', title: 'Song', fileSize: 0 }),
        track({ id: 'e', title: 'Song', fileSize: 0 })
      ],
      match,
      2
    )
    assert.deepEqual(
      groups.map((group) => group.map((item) => item.id)),
      [['a', 'b']]
    )
  })

  it('returns nothing when every field is off', () => {
    const groups = duplicateGroups(
      [track({ id: 'a', title: 'Song' }), track({ id: 'b', title: 'Song' })],
      { title: false, artist: false, duration: false, fileSize: false },
      2
    )
    assert.deepEqual(groups, [])
  })
})

describe('rankCopies', () => {
  it('puts the higher quality analysed copy first', () => {
    const ranked = rankCopies(
      [
        track({ id: 'mp3', title: 'Song', bitrate: 320, analysed: false, cueCount: 4, path: 'b.mp3' }),
        track({ id: 'wav', title: 'Song', bitrate: 1411, analysed: true, cueCount: 0, path: 'a.wav' })
      ],
      new Set(['mp3'])
    )
    assert.deepEqual(
      ranked.map((item) => item.id),
      ['wav', 'mp3']
    )
  })
})
