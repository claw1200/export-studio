import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import type { SyncDiagnostic } from '../src/preload/engine.ts'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { automaticResolutions, groupChanges, needsChoice, onlyResolution } =
  await import('../src/renderer/src/lib/syncReview.ts')

const diagnostic = (overrides: Partial<SyncDiagnostic> = {}): SyncDiagnostic => ({
  id: 'a',
  kind: 'playlist_deleted',
  severity: 'decision_required',
  entity: 'playlist',
  entityId: '1',
  name: 'raveheartz',
  formats: ['DeviceLibrary'],
  fields: ['existence'],
  evidence: [],
  detail: 'This selected playlist was deleted from the device and will be restored.',
  allowedResolutions: ['restore_from_library'],
  ...overrides
})

describe('onlyResolution', () => {
  it('names the one action a decision allows', () => {
    assert.equal(onlyResolution(diagnostic()), 'restore_from_library')
  })

  it('leaves a real choice, and information, to the person', () => {
    assert.equal(onlyResolution(diagnostic({ allowedResolutions: ['use_library', 'discard_device_edits'] })), null)
    assert.equal(onlyResolution(diagnostic({ severity: 'information' })), null)
  })
})

describe('needsChoice', () => {
  it('asks only when there is more than one action', () => {
    assert.equal(needsChoice(diagnostic()), false)
    assert.equal(needsChoice(diagnostic({ allowedResolutions: ['use_library', 'discard_device_edits'] })), true)
  })
})

describe('automaticResolutions', () => {
  it('takes every decision that has only one action', () => {
    const inspection = {
      token: 't',
      plan: null,
      diagnostics: [
        diagnostic({ id: 'one' }),
        diagnostic({ id: 'choice', allowedResolutions: ['use_library', 'discard_device_edits'] }),
        diagnostic({ id: 'info', severity: 'information' })
      ]
    }
    assert.deepEqual(automaticResolutions(inspection), { one: 'restore_from_library' })
  })
})

describe('groupChanges', () => {
  it('shows a change both databases report once, under both ids', () => {
    const changes = groupChanges([diagnostic({ id: 'legacy' }), diagnostic({ id: 'one', formats: ['OneLibrary'] })])
    assert.deepEqual(
      changes.map((change) => change.ids),
      [['legacy', 'one']]
    )
  })

  it('keeps different changes apart', () => {
    const changes = groupChanges([diagnostic({ id: 'a' }), diagnostic({ id: 'b', name: 'Pop House' })])
    assert.equal(changes.length, 2)
  })
})
