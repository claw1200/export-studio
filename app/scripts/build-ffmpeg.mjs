// Builds a minimal LGPL ffmpeg with only the codecs track conversion needs and copies it to resources/native.
// The result links nothing beyond the OS, so it runs on machines that don't have the build's libraries.
// macOS needs a C toolchain, make and lame (brew install lame).
// Windows needs an MSYS2 UCRT64 shell with make, nasm, the mingw toolchain and lame.
// Usage: node scripts/build-ffmpeg.mjs [--version 7.1]
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const isWindows = process.platform === 'win32'
const isMac = process.platform === 'darwin'
const app = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const at = args.indexOf('--version')
const version = at >= 0 ? args[at + 1] : '7.1'
const run = (cmd, argv, cwd) => execFileSync(cmd, argv, { cwd, stdio: 'inherit' })
const output = (cmd, argv) => execFileSync(cmd, argv, { encoding: 'utf8' })

const work = mkdtempSync(join(tmpdir(), 'ffmpeg-'))
const source = join(work, `ffmpeg-${version}`)
run('curl', ['-fsSL', '-o', join(work, 'ffmpeg.tar.xz'), `https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz`], work)
run('tar', ['-xf', 'ffmpeg.tar.xz'], work)

// Homebrew's and MSYS2's lame decode with mpg123, which configure's lame check doesn't link.
// Static mpg123 on Windows also needs shlwapi.
const lameDependencies = isWindows ? '--extra-libs=-lmpg123 -lshlwapi' : '--extra-libs=-lmpg123'

function staticLinkFlags() {
  if (isWindows) return ['--extra-ldflags=-static', lameDependencies]
  if (!isMac) return []
  const brewPrefix = (formula) => output('brew', ['--prefix', formula]).trim()
  // ld prefers a .dylib over a .a in the same directory, so the archives get a directory of their own.
  const archivesOnly = join(work, 'static-libs')
  mkdirSync(archivesOnly)
  for (const [formula, archive] of [
    ['lame', 'libmp3lame.a'],
    ['mpg123', 'libmpg123.a']
  ]) {
    copyFileSync(join(brewPrefix(formula), 'lib', archive), join(archivesOnly, archive))
  }
  return [
    `--extra-cflags=-I${join(brewPrefix('lame'), 'include')}`,
    `--extra-ldflags=-L${archivesOnly}`,
    lameDependencies
  ]
}

const decoders = ['mp3float', 'mp3', 'aac', 'aac_latm', 'alac', 'flac', 'vorbis', 'pcm_s16le', 'pcm_s24le', 'pcm_s32le']
decoders.push('pcm_s16be', 'pcm_s24be', 'pcm_s32be', 'pcm_f32le', 'mjpeg', 'png')
const encoders = [
  'libmp3lame',
  'aac',
  'alac',
  'flac',
  'pcm_s16le',
  'pcm_s24le',
  'pcm_s16be',
  'pcm_s24be',
  'mjpeg',
  'png'
]
const demuxers = ['mp3', 'mov', 'flac', 'wav', 'aiff', 'ogg', 'image2']
const muxers = ['mp3', 'ipod', 'flac', 'wav', 'aiff', 'image2']
const list = (flag, names) => names.map((name) => `--enable-${flag}=${name}`)

function showConfigureFailure() {
  const log = join(source, 'ffbuild', 'config.log')
  if (existsSync(log)) console.error(readFileSync(log, 'utf8').split('\n').slice(-60).join('\n'))
}

const [configure, ...configureArgs] = isWindows ? ['sh', './configure'] : ['./configure']
try {
  run(
    configure,
    [
      ...configureArgs,
      '--disable-everything',
      '--disable-autodetect',
      '--disable-doc',
      '--disable-ffplay',
      '--disable-ffprobe',
      '--enable-static',
      '--disable-shared',
      '--enable-libmp3lame',
      ...staticLinkFlags(),
      ...list('decoder', decoders),
      ...list('encoder', encoders),
      ...list('demuxer', demuxers),
      ...list('muxer', muxers),
      ...list('parser', ['mpegaudio', 'aac', 'flac']),
      ...list('protocol', ['file']),
      ...list('filter', ['aresample', 'aformat', 'anull', 'copy']),
      '--enable-bsf=aac_adtstoasc'
    ],
    source
  )
} catch (error) {
  showConfigureFailure()
  throw error
}
run('make', ['-j4', 'ffmpeg'], source)

const name = isWindows ? 'ffmpeg.exe' : 'ffmpeg'
const built = join(source, name)
if (!existsSync(built)) throw new Error(`ffmpeg was not built at ${built}`)

function linkedLibraries(binary) {
  if (isMac)
    return output('otool', ['-L', binary])
      .split('\n')
      .slice(1)
      .map((line) => line.trim().split(' ')[0])
  if (isWindows) return [...output('objdump', ['-p', binary]).matchAll(/DLL Name: (\S+)/g)].map((match) => match[1])
  return []
}

// Windows' own DLLs are never named lib*; the mingw runtime and lame are.
const isOsLibrary = (library) => (isWindows ? !/^lib/i.test(library) : /^\/(usr\/lib|System)\//.test(library))
const buildOnlyLibraries = linkedLibraries(built).filter((library) => library && !isOsLibrary(library))
if (buildOnlyLibraries.length)
  throw new Error(`ffmpeg links libraries users won't have: ${buildOnlyLibraries.join(', ')}`)

const out = join(app, 'resources', 'native')
mkdirSync(out, { recursive: true })
copyFileSync(built, join(out, name))
console.log(`[ffmpeg] ${built} -> ${join(out, name)}`)
