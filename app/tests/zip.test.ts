import assert from 'node:assert/strict'
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { after, describe, it } from 'node:test'
import { crc32 } from 'node:zlib'

import { register } from 'node:module'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const { archiveEntries, writeZip } = await import('../src/main/zip.ts')

interface ReadEntry {
  name: string
  data: Buffer
  crc: number
}

function readArchive(archive: Buffer): ReadEntry[] {
  const end = archive.length - 22
  assert.equal(archive.readUInt32LE(end), 0x06054b50)
  const count = archive.readUInt16LE(end + 10)
  let at = archive.readUInt32LE(end + 16)
  const entries: ReadEntry[] = []
  for (let i = 0; i < count; i++) {
    assert.equal(archive.readUInt32LE(at), 0x02014b50)
    const crc = archive.readUInt32LE(at + 16)
    const size = archive.readUInt32LE(at + 24)
    const nameLength = archive.readUInt16LE(at + 28)
    const extraLength = archive.readUInt16LE(at + 30)
    const offset = archive.readUInt32LE(at + 42)
    const name = archive.toString('utf8', at + 46, at + 46 + nameLength)
    assert.equal(archive.readUInt32LE(offset), 0x04034b50)
    assert.equal(archive.readUInt32LE(offset + 14), crc)
    const dataStart = offset + 30 + archive.readUInt16LE(offset + 26) + archive.readUInt16LE(offset + 28)
    entries.push({ name, crc, data: archive.subarray(dataStart, dataStart + size) })
    at += 46 + nameLength + extraLength
  }
  return entries
}

describe('archiveEntries', () => {
  it('numbers files that would land on the same name', () => {
    const names = archiveEntries([
      { path: '/a/Track.mp3', folders: [] },
      { path: '/b/track.mp3', folders: [] },
      { path: '/c/Track.mp3', folders: [] }
    ]).map((entry) => entry.name)
    assert.deepEqual(names, ['Track.mp3', 'track (2).mp3', 'Track (3).mp3'])
  })

  it('keeps one copy of a file listed twice in the same folder', () => {
    const entries = archiveEntries([
      { path: '/a/x.mp3', folders: ['Set'] },
      { path: '/a/x.mp3', folders: ['Set'] },
      { path: '/a/x.mp3', folders: ['Other'] }
    ])
    assert.deepEqual(
      entries.map((entry) => entry.name),
      ['Set/x.mp3', 'Other/x.mp3']
    )
  })

  it('keeps folder names from escaping the archive or nesting by accident', () => {
    const names = archiveEntries([
      { path: '/a/x.mp3', folders: ['..', 'AC/DC: Live'] },
      { path: '/a/y.mp3', folders: ['  '] }
    ]).map((entry) => entry.name)
    assert.deepEqual(names, ['Untitled/AC_DC_ Live/x.mp3', 'Untitled/y.mp3'])
  })
})

describe('writeZip', async () => {
  const folder = await mkdtemp(join(tmpdir(), 'zip-test-'))
  after(() => rm(folder, { recursive: true, force: true }))

  it('stores each readable file with its contents and checksum, and skips missing ones', async () => {
    const first = Buffer.from('first file')
    const second = Buffer.alloc(3 << 20, 7)
    await writeFile(join(folder, 'one.mp3'), first)
    await writeFile(join(folder, 'två.flac'), second)
    const destination = join(folder, 'out.zip')
    const progress: number[] = []

    const result = await writeZip(
      destination,
      [
        { path: join(folder, 'one.mp3'), folders: [] },
        { path: join(folder, 'missing.mp3'), folders: [] },
        { path: join(folder, 'två.flac'), folders: ['Sub'] }
      ],
      ({ done }) => progress.push(done)
    )

    assert.deepEqual(result, { written: 2, skipped: 1 })
    assert.deepEqual(progress, [1, 2, 3])
    const entries = readArchive(await readFile(destination))
    assert.deepEqual(
      entries.map((entry) => entry.name),
      ['one.mp3', 'Sub/två.flac']
    )
    assert.deepEqual(entries[0].data, first)
    assert.deepEqual(entries[1].data, second)
    for (const entry of entries) assert.equal(entry.crc, crc32(entry.data))
    assert.ok(!(await readdir(folder)).some((name) => name.endsWith('.partial')))
  })
})
