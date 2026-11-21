import { open, rename, rm, type FileHandle } from 'fs/promises'
import { basename, extname } from 'path'
import { crc32 } from 'zlib'
import type { ZipProgress, ZipSource } from '../shared/zip'

const LOCAL_HEADER_SIGNATURE = 0x04034b50
const CENTRAL_HEADER_SIGNATURE = 0x02014b50
const END_SIGNATURE = 0x06054b50
const ZIP64_END_SIGNATURE = 0x06064b50
const ZIP64_LOCATOR_SIGNATURE = 0x07064b50
const ZIP64_EXTRA_ID = 0x0001
const ZIP64_END_RECORD_REMAINDER = 44
const VERSION_WITH_ZIP64 = 45
const UTF8_NAMES_FLAG = 0x0800
const STORED = 0
const MAX_UINT16 = 0xffff
const MAX_UINT32 = 0xffffffff
const LOCAL_HEADER_BYTES = 30
const CENTRAL_HEADER_BYTES = 46
const END_RECORD_BYTES = 22
const ZIP64_END_RECORD_BYTES = 56
const ZIP64_LOCATOR_BYTES = 20
const FILE_FIELDS_IN_LOCAL_HEADER = 4
const FILE_FIELDS_IN_CENTRAL_HEADER = 6
const CRC_IN_FILE_FIELDS = 10
const DOS_FIRST_YEAR = 1980
const DOS_LAST_YEAR = 2107
const READ_CHUNK_BYTES = 1 << 20
// eslint-disable-next-line no-control-regex
const UNSAFE_NAME_CHARACTERS = /[\\/:*?"<>|\u0000-\u001f]/g
const ONLY_DOTS = /^\.*$/

export interface ArchiveEntry {
  name: string
  path: string
}

interface StoredFile {
  name: Buffer
  modified: Date
  crc: number
  size: number
  offset: number
}

export function safeFileName(name: string): string {
  const cleaned = name.replace(UNSAFE_NAME_CHARACTERS, '_').trim()
  return ONLY_DOTS.test(cleaned) ? 'Untitled' : cleaned
}

/** Names each source inside the archive, numbering clashes the way a file manager would. */
export function archiveEntries(sources: ZipSource[]): ArchiveEntry[] {
  const takenNames = new Set<string>()
  const addedSources = new Set<string>()
  const entries: ArchiveEntry[] = []
  for (const { path, folders } of sources) {
    const folder = folders.map((part) => `${safeFileName(part)}/`).join('')
    const sourceKey = `${folder}\u0000${path}`
    if (addedSources.has(sourceKey)) continue
    addedSources.add(sourceKey)
    const file = safeFileName(basename(path))
    const extension = extname(file)
    const stem = file.slice(0, file.length - extension.length)
    let name = folder + file
    // Case-insensitive, because the archive will usually be unpacked onto a case-insensitive disk.
    for (let copy = 2; takenNames.has(name.toLowerCase()); copy++) name = `${folder}${stem} (${copy})${extension}`
    takenNames.add(name.toLowerCase())
    entries.push({ name, path })
  }
  return entries
}

const clampToUint32 = (value: number): number => Math.min(value, MAX_UINT32)

function zip64Extra(values: number[]): Buffer {
  if (!values.length) return Buffer.alloc(0)
  const extra = Buffer.alloc(4 + 8 * values.length)
  extra.writeUInt16LE(ZIP64_EXTRA_ID, 0)
  extra.writeUInt16LE(8 * values.length, 2)
  values.forEach((value, i) => extra.writeBigUInt64LE(BigInt(value), 4 + 8 * i))
  return extra
}

function writeDosTimestamp(buffer: Buffer, at: number, date: Date): void {
  const year = Math.min(Math.max(date.getFullYear(), DOS_FIRST_YEAR), DOS_LAST_YEAR)
  buffer.writeUInt16LE((date.getHours() << 11) | (date.getMinutes() << 5) | (date.getSeconds() >> 1), at)
  buffer.writeUInt16LE(((year - DOS_FIRST_YEAR) << 9) | ((date.getMonth() + 1) << 5) | date.getDate(), at + 2)
}

/** The run of fields that local and central headers share, from "version needed" to "extra length". */
function writeFileFields(buffer: Buffer, at: number, file: StoredFile, extra: Buffer): void {
  buffer.writeUInt16LE(VERSION_WITH_ZIP64, at)
  buffer.writeUInt16LE(UTF8_NAMES_FLAG, at + 2)
  buffer.writeUInt16LE(STORED, at + 4)
  writeDosTimestamp(buffer, at + 6, file.modified)
  buffer.writeUInt32LE(file.crc, at + CRC_IN_FILE_FIELDS)
  buffer.writeUInt32LE(clampToUint32(file.size), at + 14)
  buffer.writeUInt32LE(clampToUint32(file.size), at + 18)
  buffer.writeUInt16LE(file.name.length, at + 22)
  buffer.writeUInt16LE(extra.length, at + 24)
}

const sizeOverflow = (file: StoredFile): number[] => (file.size >= MAX_UINT32 ? [file.size, file.size] : [])

function localHeader(file: StoredFile): Buffer {
  const extra = zip64Extra(sizeOverflow(file))
  const header = Buffer.alloc(LOCAL_HEADER_BYTES)
  header.writeUInt32LE(LOCAL_HEADER_SIGNATURE, 0)
  writeFileFields(header, FILE_FIELDS_IN_LOCAL_HEADER, file, extra)
  return Buffer.concat([header, file.name, extra])
}

function centralHeader(file: StoredFile): Buffer {
  const offsetOverflow = file.offset >= MAX_UINT32 ? [file.offset] : []
  const extra = zip64Extra([...sizeOverflow(file), ...offsetOverflow])
  const header = Buffer.alloc(CENTRAL_HEADER_BYTES)
  header.writeUInt32LE(CENTRAL_HEADER_SIGNATURE, 0)
  header.writeUInt16LE(VERSION_WITH_ZIP64, 4)
  writeFileFields(header, FILE_FIELDS_IN_CENTRAL_HEADER, file, extra)
  header.writeUInt32LE(clampToUint32(file.offset), 42)
  return Buffer.concat([header, file.name, extra])
}

function endRecords(count: number, directoryOffset: number, directorySize: number): Buffer {
  const needsZip64 = count >= MAX_UINT16 || directoryOffset >= MAX_UINT32 || directorySize >= MAX_UINT32
  const records: Buffer[] = []
  if (needsZip64) {
    const record = Buffer.alloc(ZIP64_END_RECORD_BYTES)
    record.writeUInt32LE(ZIP64_END_SIGNATURE, 0)
    record.writeBigUInt64LE(BigInt(ZIP64_END_RECORD_REMAINDER), 4)
    record.writeUInt16LE(VERSION_WITH_ZIP64, 12)
    record.writeUInt16LE(VERSION_WITH_ZIP64, 14)
    record.writeBigUInt64LE(BigInt(count), 24)
    record.writeBigUInt64LE(BigInt(count), 32)
    record.writeBigUInt64LE(BigInt(directorySize), 40)
    record.writeBigUInt64LE(BigInt(directoryOffset), 48)
    const locator = Buffer.alloc(ZIP64_LOCATOR_BYTES)
    locator.writeUInt32LE(ZIP64_LOCATOR_SIGNATURE, 0)
    locator.writeBigUInt64LE(BigInt(directoryOffset + directorySize), 8)
    locator.writeUInt32LE(1, 16)
    records.push(record, locator)
  }
  const end = Buffer.alloc(END_RECORD_BYTES)
  end.writeUInt32LE(END_SIGNATURE, 0)
  end.writeUInt16LE(Math.min(count, MAX_UINT16), 8)
  end.writeUInt16LE(Math.min(count, MAX_UINT16), 10)
  end.writeUInt32LE(clampToUint32(directorySize), 12)
  end.writeUInt32LE(clampToUint32(directoryOffset), 16)
  records.push(end)
  return Buffer.concat(records)
}

class ArchiveWriter {
  private position = 0
  private stored: StoredFile[] = []
  private readonly output: FileHandle

  constructor(output: FileHandle) {
    this.output = output
  }

  private async append(data: Uint8Array): Promise<void> {
    await this.output.write(data, 0, data.length, this.position)
    this.position += data.length
  }

  /** Copies one file in, or returns false when it can't be opened. */
  async add(entry: ArchiveEntry): Promise<boolean> {
    const input = await open(entry.path, 'r').catch(() => null)
    if (!input) return false
    try {
      const info = await input.stat()
      if (!info.isFile()) return false
      const file: StoredFile = {
        name: Buffer.from(entry.name),
        modified: info.mtime,
        crc: 0,
        size: info.size,
        offset: this.position
      }
      await this.append(localHeader(file))
      let copied = 0
      for await (const chunk of input.createReadStream({ highWaterMark: READ_CHUNK_BYTES, autoClose: false })) {
        file.crc = crc32(chunk, file.crc)
        copied += chunk.length
        await this.append(chunk)
      }
      if (copied !== file.size) throw new Error(`${entry.path} changed while it was being archived`)
      const crc = Buffer.alloc(4)
      crc.writeUInt32LE(file.crc)
      await this.output.write(crc, 0, 4, file.offset + FILE_FIELDS_IN_LOCAL_HEADER + CRC_IN_FILE_FIELDS)
      this.stored.push(file)
      return true
    } finally {
      await input.close()
    }
  }

  async finish(): Promise<void> {
    const directoryOffset = this.position
    const directory = Buffer.concat(this.stored.map(centralHeader))
    await this.append(directory)
    await this.append(endRecords(this.stored.length, directoryOffset, directory.length))
  }
}

/**
 * Stores the sources uncompressed, since audio formats are already compressed.
 * Builds a sibling file and renames it at the end, so a failed export leaves nothing half-written.
 */
export async function writeZip(
  destination: string,
  sources: ZipSource[],
  onProgress: (progress: ZipProgress) => void
): Promise<{ written: number; skipped: number }> {
  const entries = archiveEntries(sources)
  const partial = `${destination}.partial`
  const output = await open(partial, 'w')
  let written = 0
  try {
    const writer = new ArchiveWriter(output)
    for (const [index, entry] of entries.entries()) {
      if (await writer.add(entry)) written++
      onProgress({ done: index + 1, total: entries.length })
    }
    await writer.finish()
    await output.close()
    await rename(partial, destination)
  } catch (error) {
    await output.close().catch(() => undefined)
    await rm(partial, { force: true })
    throw error
  }
  return { written, skipped: entries.length - written }
}
