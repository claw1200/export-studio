/** Lets node:test load extensionless imports from the renderer sources. */
const libRoot = new URL('../src/renderer/src/lib/', import.meta.url)

function withExtension(specifier) {
  return /\.[a-z]+$/i.test(specifier) ? specifier : `${specifier}.ts`
}

export function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('$lib/')) {
    return nextResolve(new URL(withExtension(specifier.slice('$lib/'.length)), libRoot).href, context)
  }
  if (specifier.startsWith('.') && !/\.[a-z]+$/i.test(specifier)) {
    return nextResolve(`${specifier}.ts`, context)
  }
  return nextResolve(specifier, context)
}
