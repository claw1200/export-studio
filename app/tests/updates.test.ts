import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

const { compareVersions, findUpdate } = await import('../src/shared/updates.ts')
type Release = import('../src/shared/updates.ts').Release

function release(tag: string, prerelease = false, draft = false): Release {
  return { tag_name: tag, html_url: `https://example.com/${tag}`, prerelease, draft }
}

describe('compareVersions', () => {
  it('orders by semver precedence', () => {
    const sorted = ['1.0.0', '0.1.0-alpha.10', '0.1.0', '0.1.0-alpha.2', '0.1.0-beta', '0.2.0', 'v0.1.0-alpha.9']
    sorted.sort(compareVersions)
    assert.deepEqual(sorted, [
      '0.1.0-alpha.2',
      'v0.1.0-alpha.9',
      '0.1.0-alpha.10',
      '0.1.0-beta',
      '0.1.0',
      '0.2.0',
      '1.0.0'
    ])
  })
})

describe('findUpdate', () => {
  const releases = [
    release('0.1.0-alpha.23', true),
    release('0.1.0-alpha.22'),
    release('0.1.0-alpha.21'),
    release('0.1.0-alpha.19', true),
    release('0.1.0-alpha.24', false, true)
  ]

  it('offers only normal releases when the running version is a normal release', () => {
    assert.deepEqual(findUpdate('0.1.0-alpha.21', releases), {
      version: '0.1.0-alpha.22',
      url: 'https://example.com/0.1.0-alpha.22'
    })
  })

  it('offers pre-releases when the running version is a pre-release', () => {
    assert.equal(findUpdate('0.1.0-alpha.19', releases)?.version, '0.1.0-alpha.23')
  })

  it('treats a version with no release as a normal release', () => {
    assert.equal(findUpdate('0.1.0-alpha.1', releases)?.version, '0.1.0-alpha.22')
  })

  it('ignores drafts and finds nothing when already up to date', () => {
    assert.equal(findUpdate('0.1.0-alpha.23', releases), null)
    assert.equal(findUpdate('0.1.0', releases), null)
  })
})
