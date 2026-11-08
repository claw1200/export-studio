export const RELEASES_URL = 'https://api.github.com/repos/claw1200/export-studio/releases?per_page=100'

/** The fields used from GitHub's list-releases response. */
export interface Release {
  tag_name: string
  html_url: string
  prerelease: boolean
  draft: boolean
}

export interface Update {
  version: string
  url: string
}

interface Version {
  core: number[]
  pre: string[]
}

function parseVersion(text: string): Version | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(text.trim())
  if (!match) return null
  return { core: [+match[1], +match[2], +match[3]], pre: match[4] ? match[4].split('.') : [] }
}

function compareIdentifiers(a: string, b: string): number {
  const aNumeric = /^\d+$/.test(a)
  const bNumeric = /^\d+$/.test(b)
  if (aNumeric && bNumeric) return +a - +b
  if (aNumeric !== bNumeric) return aNumeric ? -1 : 1
  return a < b ? -1 : a > b ? 1 : 0
}

/** Semver precedence: negative when `a` is older than `b`. */
export function compareVersions(a: string, b: string): number {
  const va = parseVersion(a)
  const vb = parseVersion(b)
  if (!va || !vb) return 0
  for (let i = 0; i < 3; i++) {
    if (va.core[i] !== vb.core[i]) return va.core[i] - vb.core[i]
  }
  if (!va.pre.length || !vb.pre.length) return vb.pre.length - va.pre.length
  for (let i = 0; i < Math.min(va.pre.length, vb.pre.length); i++) {
    const order = compareIdentifiers(va.pre[i], vb.pre[i])
    if (order) return order
  }
  return va.pre.length - vb.pre.length
}

function sameVersion(tag: string, version: string): boolean {
  return tag.replace(/^v/, '') === version.replace(/^v/, '')
}

/**
 * The newest release that is newer than `current`, if any. Pre-releases only count when the running
 * version was itself published as a pre-release on GitHub; the tag alone doesn't say, since some
 * normal releases carry `-alpha` tags. A version with no release (a dev build) only sees normal releases.
 */
export function findUpdate(current: string, releases: readonly Release[]): Update | null {
  if (!parseVersion(current)) return null
  const published = releases.filter((r) => !r.draft && parseVersion(r.tag_name))
  const includePrereleases = published.find((r) => sameVersion(r.tag_name, current))?.prerelease ?? false
  let newest: Release | null = null
  for (const release of published) {
    if (release.prerelease && !includePrereleases) continue
    if (compareVersions(release.tag_name, newest?.tag_name ?? current) > 0) newest = release
  }
  return newest && { version: newest.tag_name.replace(/^v/, ''), url: newest.html_url }
}
