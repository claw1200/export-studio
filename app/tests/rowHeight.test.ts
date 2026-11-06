import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const {
  ROW_HEIGHT_DEFAULT,
  ROW_HEIGHT_MAX,
  ROW_HEIGHT_MIN,
  clampRowHeight,
  readRowHeight,
  rowFontSize,
  stackedRowHeight
} = await import('../src/renderer/src/lib/browserColumns.ts')

describe('readRowHeight', () => {
  it('keeps the original 32px rows when nothing is stored', () => {
    assert.equal(ROW_HEIGHT_DEFAULT, 32)
    assert.equal(readRowHeight(null), 32)
    assert.equal(readRowHeight(''), 32)
  })

  it('falls back to the default for values that are not numbers', () => {
    assert.equal(readRowHeight('tall'), ROW_HEIGHT_DEFAULT)
    assert.equal(readRowHeight('NaN'), ROW_HEIGHT_DEFAULT)
  })

  it('reads a stored height', () => {
    assert.equal(readRowHeight('40'), 40)
  })
})

describe('clampRowHeight', () => {
  it('keeps heights inside the slider range', () => {
    assert.equal(clampRowHeight(4), ROW_HEIGHT_MIN)
    assert.equal(clampRowHeight(500), ROW_HEIGHT_MAX)
    assert.equal(clampRowHeight(-Infinity), ROW_HEIGHT_DEFAULT)
  })

  it('rounds to the slider step', () => {
    assert.equal(clampRowHeight(33), 34)
    assert.equal(clampRowHeight(32.9), 32)
  })
})

describe('rowFontSize', () => {
  it('matches the original text size at the default height', () => {
    assert.equal(rowFontSize(ROW_HEIGHT_DEFAULT), 12.5)
  })

  it('grows with the row and stays readable when small', () => {
    assert.ok(rowFontSize(ROW_HEIGHT_MAX) > rowFontSize(ROW_HEIGHT_DEFAULT))
    assert.ok(rowFontSize(ROW_HEIGHT_MIN) >= 10)
    assert.ok(rowFontSize(ROW_HEIGHT_MIN) < rowFontSize(ROW_HEIGHT_DEFAULT))
  })
})

describe('stackedRowHeight', () => {
  it('makes alternative-style rows taller than the chosen height', () => {
    assert.equal(stackedRowHeight(ROW_HEIGHT_DEFAULT), 48)
    assert.equal(stackedRowHeight(ROW_HEIGHT_MIN), 36)
    assert.equal(stackedRowHeight(ROW_HEIGHT_MAX), 84)
  })

  it('clamps the chosen height first', () => {
    assert.equal(stackedRowHeight(1000), stackedRowHeight(ROW_HEIGHT_MAX))
  })
})
