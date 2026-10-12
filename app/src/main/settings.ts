import { app, ipcMain } from 'electron'
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'fs'
import { dirname, join } from 'path'

/** Preferences file in the user-data directory, which stays in place across updates. */
const FILE_NAME = 'settings.json'

type Bag = Record<string, string>

let values: Bag = Object.create(null)
let path = ''
let existed = false
let dirty = false
let pending: ReturnType<typeof setTimeout> | null = null

function bagFrom(raw: unknown): Bag {
  const bag: Bag = Object.create(null)
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return bag
  for (const [key, value] of Object.entries(raw)) {
    if (key && typeof value === 'string') bag[key] = value
  }
  return bag
}

function writeNow(): void {
  if (!path) return
  mkdirSync(dirname(path), { recursive: true })
  const tmp = `${path}.${process.pid}.tmp`
  writeFileSync(tmp, `${JSON.stringify({ ...values }, null, 2)}\n`)
  renameSync(tmp, path)
  existed = true
}

function schedule(): void {
  dirty = true
  if (pending) clearTimeout(pending)
  pending = setTimeout(() => {
    pending = null
    try {
      writeNow()
      dirty = false
    } catch (err) {
      console.error('[settings] failed to write', err)
    }
  }, 150)
}

/** Loads the preferences file. A missing file is a first launch, not an empty one. */
export function loadSettings(): void {
  path = join(app.getPath('userData'), FILE_NAME)
  try {
    values = bagFrom(JSON.parse(readFileSync(path, 'utf8')))
    existed = true
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code
    if (code !== 'ENOENT') console.error('[settings] failed to read', err)
    values = Object.create(null)
    existed = false
  }
}

export function registerSettings(): void {
  ipcMain.on('settings:load', (event) => {
    event.returnValue = { exists: existed, values: { ...values } }
  })
  ipcMain.on('settings:set', (_event, key: unknown, value: unknown) => {
    if (typeof key !== 'string' || !key || typeof value !== 'string') return
    values[key] = value
    schedule()
  })
  ipcMain.on('settings:replace', (_event, next: unknown) => {
    values = bagFrom(next)
    dirty = true
    if (pending) {
      clearTimeout(pending)
      pending = null
    }
    try {
      writeNow()
      dirty = false
    } catch (err) {
      console.error('[settings] failed to write', err)
    }
  })
}

/** Writes any change still waiting on the debounce, including during quit. */
export function flushSettings(): void {
  if (!dirty) return
  if (pending) {
    clearTimeout(pending)
    pending = null
  }
  try {
    writeNow()
    dirty = false
  } catch (err) {
    console.error('[settings] failed to write', err)
  }
}
