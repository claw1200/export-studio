import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { COLUMNS, defaultHidden, defaultOrder, readColumnPrefs } =
  await import('../src/renderer/src/lib/browserColumns.ts')

describe('readColumnPrefs', () => {
  it('uses the default order and hidden columns when nothing is stored', () => {
    const prefs = readColumnPrefs(null)
    assert.deepEqual(prefs.order, defaultOrder())
    assert.deepEqual(prefs.hidden, defaultHidden())
    assert.equal(prefs.order[0], 'index')
    assert.equal(prefs.order[1], 'compat')
  })

  it('places hardware after the index when a layout-2 order is loaded', () => {
    const prefs = readColumnPrefs(
      JSON.stringify({
        layout: 2,
        order: ['title', 'artist', 'index', 'bpm', 'compat'],
        hidden: ['album'],
        widths: { title: 99999 }
      })
    )
    assert.equal(prefs.order[prefs.order.indexOf('index') + 1], 'compat')
    assert.equal(prefs.widths.title, COLUMNS.find((column) => column.id === 'title').max)
    assert.deepEqual(prefs.hidden, ['album'])
  })

  it('keeps a layout-3 order, including hardware away from the index', () => {
    const prefs = readColumnPrefs(
      JSON.stringify({
        layout: 3,
        order: ['title', 'compat', 'index'],
        hidden: ['album']
      })
    )
    assert.deepEqual(prefs.order.slice(0, 3), ['title', 'compat', 'index'])
    assert.ok(prefs.order.includes('artist'))
    assert.deepEqual(prefs.hidden, ['album'])
  })

  it('ignores a broken value and an older layout’s order', () => {
    assert.deepEqual(readColumnPrefs('{').order, defaultOrder())
    const older = readColumnPrefs(
      JSON.stringify({ layout: 1, order: ['title'], hidden: ['title'], widths: { bpm: 80 } })
    )
    assert.deepEqual(older.order, defaultOrder())
    assert.deepEqual(older.hidden, defaultHidden())
    assert.equal(older.widths.bpm, 80)
  })
})
