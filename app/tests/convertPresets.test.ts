import assert from 'node:assert/strict'
import { test } from 'node:test'
import { DEFAULT_PRESETS, parsePresets, presetLabel } from '../src/renderer/src/lib/convertPresets.ts'

test('defaults label lossless by depth and rate, lossy by bitrate', () => {
  assert.deepEqual(DEFAULT_PRESETS.map(presetLabel), [
    'WAV 16-bit 48 kHz',
    'FLAC 16-bit 48 kHz',
    'MP3 320 kbps',
    'AAC 256 kbps',
    'ALAC 16-bit 48 kHz'
  ])
})

test('parsePresets falls back to defaults and drops invalid or duplicate entries', () => {
  assert.equal(parsePresets(null), DEFAULT_PRESETS)
  assert.equal(parsePresets('nope'), DEFAULT_PRESETS)
  const list = parsePresets(
    JSON.stringify([{ format: 'mp3', bitrate: 320 }, { format: 'mp3', bitrate: 320 }, { format: 'mp3', bitrate: 1 }, { format: 'ogg' }])
  )
  assert.deepEqual(list, [{ format: 'mp3', bitrate: 320 }])
})
