import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { emptyProblemChecks, issueText, localIssues, problemRows } from '../src/renderer/src/lib/library/problems.ts'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { introduceViews } = await import('../src/renderer/src/lib/library/defaults.ts')
import type { ProblemChecks, Track, TrackIssue, View } from '../src/renderer/src/lib/types.ts'

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
    fileType: 'MP3',
    storedType: 'MP3',
    path: '/music/tune.mp3',
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

const all = emptyProblemChecks()

describe('localIssues', () => {
  it('names both sides when rekordbox still calls a wav an mp3', () => {
    const issues = localIssues(
      track({ id: '1', title: 'Night', path: '/music/night.wav', fileType: 'MP3', storedType: 'MP3' })
    )
    assert.equal(issueText(issues), 'Rekordbox says MP3, the file is WAV')
  })

  it('reports a missing file, and the type when the path disagrees', () => {
    const issues = localIssues(
      track({ id: '1', title: 'Night', path: '/music/night.wav', storedType: 'MP3', missing: true })
    )
    assert.equal(issueText(issues), 'File missing · Rekordbox says MP3, the file is WAV')
  })

  it('leaves streaming tracks out', () => {
    assert.deepEqual(localIssues(track({ id: '1', title: 'Stream', streaming: true, path: 'tidal:1' })), [])
  })
})

describe('problemRows', () => {
  const wav = track({ id: 'wav', title: 'Bravo', path: '/music/b.wav', storedType: 'MP3' })
  const gone = track({ id: 'gone', title: 'Alpha', path: '/music/a.mp3', storedType: 'MP3', missing: true })
  const fine = track({ id: 'fine', title: 'Fine' })

  it('filters by the checks that are on and orders severe problems first', () => {
    const checks: ProblemChecks = { ...all, missing: false }
    const rows = problemRows([wav, gone, fine], checks, {})
    assert.deepEqual(
      rows.map((row) => row.track.id),
      ['wav']
    )
    assert.equal(rows[0].text, 'Rekordbox says MP3, the file is WAV')
  })

  it('uses a probe result instead of the local guess', () => {
    const probed: Record<string, TrackIssue[]> = {
      fine: [{ code: 'unreadable', detail: "File can't be read" }]
    }
    const rows = problemRows([fine, wav], all, probed)
    assert.deepEqual(
      rows.map((row) => [row.track.id, row.text]),
      [
        ['fine', "File can't be read"],
        ['wav', 'Rekordbox says MP3, the file is WAV']
      ]
    )
  })

  it('drops a track whose probe found nothing', () => {
    const rows = problemRows([wav], all, { wav: [] })
    assert.equal(rows.length, 0)
  })
})

describe('introduceViews', () => {
  const old: View = {
    id: 'default:duplicates',
    name: 'Suspected duplicates',
    type: 'duplicates',
    match: { title: true, artist: true, duration: true, fileSize: false },
    durationSlack: 2
  }

  it('adds Damaged tracks on the first launch', () => {
    const seeded = introduceViews([], '', false, [])
    assert.ok(seeded.views.some((view) => view.id === 'default:problems' && view.name === 'Damaged tracks'))
    assert.ok(seeded.introduced.includes('default:problems'))
    const later = introduceViews(seeded.views, '/Users/me/Music', true, seeded.introduced)
    assert.equal(
      later.views.some((view) => view.id === 'default:music'),
      false
    )
  })

  it('adds Damaged tracks once for a library that was seeded before this view existed', () => {
    const seeded = introduceViews([old], '', true, [])
    assert.equal(seeded.views[0].id, 'default:problems')
    assert.equal(seeded.views.filter((view) => view.id === 'default:duplicates').length, 1)
    const again = introduceViews(
      seeded.views.filter((view) => view.id !== 'default:problems'),
      '',
      true,
      seeded.introduced
    )
    assert.equal(
      again.views.some((view) => view.id === 'default:problems'),
      false
    )
  })

  it('does not bring back a built-in the user already deleted', () => {
    const seeded = introduceViews([], '', true, [])
    assert.equal(
      seeded.views.some((view) => view.id === 'default:duplicates'),
      false
    )
    assert.ok(seeded.views.some((view) => view.id === 'default:problems'))
  })
})
