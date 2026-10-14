import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { isDefaultRekordboxTrack } from '../src/renderer/src/lib/library/factory.ts'

describe('default rekordbox tracks', () => {
  it('matches factory sampler presets', () => {
    assert.equal(
      isDefaultRekordboxTrack(
        '/Users/c.wood/Music/rekordbox/Sampler/GROOVE CIRCUIT/PRESET/4-Floor Breaks Kit/Breaks2.wav'
      ),
      true
    )
    assert.equal(
      isDefaultRekordboxTrack('C:\\Users\\c.wood\\Music\\rekordbox\\Sampler\\OSC_SAMPLER\\PRESET ONESHOT\\Kick.wav'),
      true
    )
    assert.equal(
      isDefaultRekordboxTrack('/Users/c.wood/Music/rekordbox/Sampler/OSC_SAMPLER(1)/PRESET ONESHOT/Hat.wav'),
      true
    )
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/rekordbox/Sampler/MERGE FX/Noise.wav'), true)
  })

  it('leaves recorded samples and the rest of the collection', () => {
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/rekordbox/Sampler/Capture/Loop.wav'), false)
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/rekordbox/Sampler/GROOVE CIRCUIT/MyLoop.wav'), false)
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/rekordbox/Recording/Unknown Artist/Take.wav'), false)
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/Library/Track.wav'), false)
    assert.equal(isDefaultRekordboxTrack('/Users/c.wood/Music/Presets/Kick.wav'), false)
    assert.equal(isDefaultRekordboxTrack(''), false)
  })
})
