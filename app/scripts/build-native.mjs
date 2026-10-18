// Builds the Rust audio engine and copies it to resources/native/es_node.node.
// Usage: node scripts/build-native.mjs [--debug] [--target <rust-triple>]
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const native = resolve(app, '..', 'native')
const args = process.argv.slice(2)
const debug = args.includes('--debug')
const targetAt = args.indexOf('--target')
const target = targetAt >= 0 ? args[targetAt + 1] : process.env.ES_NATIVE_TARGET

const cargo = [
  'build',
  '--locked',
  '-p',
  'es-node',
  '-p',
  'es-usb',
  ...(debug ? [] : ['--release']),
  ...(target ? ['--target', target] : [])
]
execFileSync('cargo', cargo, { cwd: native, stdio: 'inherit' })

const platform = target
  ? target.includes('windows')
    ? 'win32'
    : target.includes('apple')
      ? 'darwin'
      : 'linux'
  : process.platform
const file = platform === 'win32' ? 'es_node.dll' : platform === 'darwin' ? 'libes_node.dylib' : 'libes_node.so'
const built = join(native, 'target', ...(target ? [target] : []), debug ? 'debug' : 'release', file)
const out = join(app, 'resources', 'native')
mkdirSync(out, { recursive: true })
// A new file rather than an overwrite: macOS kills any process that loads a signed library
// rewritten in place, since its cached signature no longer matches.
rmSync(join(out, 'es_node.node'), { force: true })
copyFileSync(built, join(out, 'es_node.node'))
console.log(`[native] ${built} -> ${join(out, 'es_node.node')}`)

const usbName = platform === 'win32' ? 'es-usb.exe' : 'es-usb'
const usbBuilt = join(native, 'target', ...(target ? [target] : []), debug ? 'debug' : 'release', usbName)
copyFileSync(usbBuilt, join(out, usbName))
console.log(`[native] ${usbBuilt} -> ${join(out, usbName)}`)
