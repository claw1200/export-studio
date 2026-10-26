import { execFileSync } from 'node:child_process'
import { resolve, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const native = join(root, 'native')
const sdk = join(root, 'es-rekordbox-sdk')
const environment = { ...process.env, RB_LIBRARY_TEST: '1', RB_LITE_TEST: '1' }

execFileSync(process.execPath, [join(sdk, 'check-layers.mjs')], {
  stdio: 'inherit',
  env: environment
})

// Same profile and feature set as `cargo test --workspace` below, so the native workspace compiles once.
execFileSync('cargo', ['build', '--locked', '--workspace', '--all-targets', '--profile', 'test'], {
  cwd: native,
  stdio: 'inherit',
  env: environment
})
environment.EXPORTSTUDIO_USB_EXPORT = join(
  native,
  'target',
  'debug',
  process.platform === 'win32' ? 'es-usb.exe' : 'es-usb'
)
// Run every test binary in both workspaces before failing, so one CI run lists every failure.
let failed = false
for (const cwd of [sdk, native]) {
  try {
    execFileSync('cargo', ['test', '--locked', '--workspace', '--no-fail-fast'], {
      cwd,
      stdio: 'inherit',
      env: environment
    })
  } catch {
    failed = true
  }
}
if (failed) process.exit(1)
