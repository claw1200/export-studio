import { execFileSync } from 'node:child_process'
import { resolve, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const vendor = join(root, 'vendor', 'usb-sync')
const native = join(root, 'native')
const environment = { ...process.env, ES_DB_TEST: '1', RB_LITE_TEST: '1' }

execFileSync('cargo', ['build', '--locked', '-p', 'es-usb'], {
  cwd: vendor,
  stdio: 'inherit',
  env: environment
})
environment.EXPORTSTUDIO_USB_EXPORT = join(
  vendor,
  'target',
  'debug',
  process.platform === 'win32' ? 'es-usb.exe' : 'es-usb'
)
for (const directory of [vendor, native]) {
  execFileSync('cargo', ['test', '--locked', '--workspace'], {
    cwd: directory,
    stdio: 'inherit',
    env: environment
  })
}
