/** Whether none, some, or all of a set of tracks already sit in a playlist. */
export type Membership = 'none' | 'some' | 'all'

export function membership(contents: ReadonlySet<string>, ids: readonly string[]): Membership {
  let found = 0
  for (const id of ids) if (contents.has(id)) found++
  return found === 0 ? 'none' : found === ids.length ? 'all' : 'some'
}
