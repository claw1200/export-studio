import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { syncFraction, syncPhaseLabel, syncStoppable } = await import('../src/renderer/src/lib/syncProgress.ts')

describe('syncPhaseLabel', () => {
  it('says the sync is preparing before the first event', () => {
    assert.equal(syncPhaseLabel(null), 'Preparing sync')
  })

  it('counts the track being copied and names it', () => {
    assert.equal(syncPhaseLabel({ phase: 'copy', done: 2, total: 10, name: 'Track' }), 'Copying 3 of 10 · Track')
  })

  it('never counts past the last track', () => {
    assert.equal(syncPhaseLabel({ phase: 'copy', done: 10, total: 10 }), 'Copying 10 of 10')
  })

  it('describes a track phase without a count', () => {
    assert.equal(syncPhaseLabel({ phase: 'checking', done: 0, total: 0 }), 'Checking the USB')
  })

  it('counts the files already published', () => {
    const label = syncPhaseLabel({ phase: 'publishing', done: 412, total: 1180, unit: 'files' })
    assert.equal(label, `Publishing 412 of ${(1180).toLocaleString()} files`)
  })

  it('names the stages after publishing', () => {
    assert.equal(syncPhaseLabel({ phase: 'cleanup' }), 'Tidying up')
  })

  it('falls back for a phase it does not know', () => {
    assert.equal(syncPhaseLabel({ phase: 'something-new' }), 'Syncing')
  })
})

describe('syncFraction', () => {
  it('is indeterminate without a total', () => {
    assert.equal(syncFraction({ phase: 'writing', done: 0, total: 0 }), null)
    assert.equal(syncFraction(null), null)
  })

  it('adds the progress through the current file to the tracks before it', () => {
    const fraction = syncFraction({ phase: 'copy', done: 1, total: 4, file: { done: 50, total: 100, unit: 'bytes' } })
    assert.equal(fraction, 1.5 / 4)
  })

  it('ignores a file with no known size', () => {
    assert.equal(syncFraction({ phase: 'copy', done: 1, total: 4, file: { done: 0, total: 0, unit: 'bytes' } }), 1 / 4)
  })

  it('never passes the end', () => {
    assert.equal(syncFraction({ phase: 'publishing', done: 12, total: 10, unit: 'files' }), 1)
    assert.equal(syncFraction({ phase: 'copy', done: 3, total: 4, file: { done: 200, total: 100, unit: 'bytes' } }), 1)
  })
})

describe('syncStoppable', () => {
  it('can stop a sync until files start going into place', () => {
    assert.equal(syncStoppable(null), true)
    assert.equal(syncStoppable({ phase: 'copy', done: 3, total: 10 }), true)
    assert.equal(syncStoppable({ phase: 'verifying' }), true)
  })

  it('lets a sync finish once it is publishing', () => {
    for (const phase of ['publishing', 'cleanup', 'done', 'eject']) assert.equal(syncStoppable({ phase }), false)
  })
})
