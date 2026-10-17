import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { cueColor, HOT_CUE_DEFAULT, LOOP_DEFAULT, hotCuePaletteColor } from '../src/renderer/src/lib/format.ts'
import type { Cue } from '../src/renderer/src/lib/types.ts'

const cue: Cue = {
  id: 'cue',
  kind: 'hot',
  slot: 1,
  start: 1000,
  end: null,
  color: null,
  colorIndex: null,
  colorCode: 0,
  comment: '',
  rgb: '#ff0017'
}

describe('cueColor', () => {
  it('does not turn exported slot RGB into an explicit colour', () => {
    assert.equal(cueColor(cue), HOT_CUE_DEFAULT)
    assert.equal(cueColor({ ...cue, colorIndex: 0 }), HOT_CUE_DEFAULT)
    assert.equal(cueColor({ ...cue, end: 2000 }), LOOP_DEFAULT)
  })

  it('uses an imported or selected palette index instead of device RGB', () => {
    assert.equal(cueColor({ ...cue, colorIndex: 46, colorCode: 46, rgb: '#ff005c' }), hotCuePaletteColor(46))
  })

  it('does not revive old analysis colours after clearing a selection', () => {
    assert.equal(cueColor({ ...cue, colorCode: 46, rgb: '#ff005c' }), HOT_CUE_DEFAULT)
  })

  it('keeps RGB fallback when the analysis palette code is unavailable', () => {
    assert.equal(cueColor({ ...cue, colorCode: null }), '#ff0017')
    assert.equal(cueColor({ ...cue, colorCode: undefined }), '#ff0017')
  })

  it('uses RGB for a selected index outside the desktop palette', () => {
    assert.equal(cueColor({ ...cue, colorIndex: 64, colorCode: 64, rgb: '#ffffff' }), '#ffffff')
  })
})
