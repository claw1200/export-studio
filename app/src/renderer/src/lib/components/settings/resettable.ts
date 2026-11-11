export type Resettable = { isDefault: () => boolean; reset: () => void }

export function setting<T>(current: () => T, initial: T, set: (value: T) => void): Resettable {
  return { isDefault: () => current() === initial, reset: () => set(initial) }
}

export function allOf(...settings: Resettable[]): Resettable {
  return {
    isDefault: () => settings.every((s) => s.isDefault()),
    reset: () => settings.forEach((s) => s.reset())
  }
}
