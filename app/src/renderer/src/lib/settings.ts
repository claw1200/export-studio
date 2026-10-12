/** Preferences kept in the user-data directory so an update does not reset them. */

type Snapshot = { exists: boolean; values: Record<string, string> }

const store = new Map<string, string>()

function legacy(): Record<string, string> {
  const values: Record<string, string> = {}
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i)
      if (!key) continue
      const value = localStorage.getItem(key)
      if (value != null) values[key] = value
    }
  } catch {
    // A locked-down page can refuse localStorage; the file store still works.
  }
  return values
}

function boot(): void {
  if (typeof window === 'undefined' || typeof window.api?.loadSettings !== 'function') return
  const snapshot: Snapshot = window.api.loadSettings()
  const values = snapshot.exists ? snapshot.values : legacy()
  for (const [key, value] of Object.entries(values)) {
    if (typeof value === 'string') store.set(key, value)
  }
  // Missing file means this origin's localStorage has not been copied yet.
  if (!snapshot.exists) window.api.replaceSettings(Object.fromEntries(store))
}

boot()

export function getSetting(key: string): string | null {
  return store.get(key) ?? null
}

export function setSetting(key: string, value: string): void {
  store.set(key, value)
  window.api.writeSetting(key, value)
}
