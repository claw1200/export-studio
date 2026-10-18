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

execFileSync('cargo', ['build', '--locked', '-p', 'es-usb'], {
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
execFileSync('cargo', ['test', '--locked', '--workspace'], {
  cwd: sdk,
  stdio: 'inherit',
  env: environment
})
execFileSync('cargo', ['test', '--locked', '--workspace'], {
  cwd: native,
  stdio: 'inherit',
  env: environment
})
