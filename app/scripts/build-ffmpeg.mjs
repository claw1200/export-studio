// Builds a minimal LGPL ffmpeg with only the codecs track conversion needs and copies it to
// resources/native/ffmpeg. Needs a C toolchain, make, pkg-config and libmp3lame (brew install lame).
// Usage: node scripts/build-ffmpeg.mjs [--version 7.1]
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const at = args.indexOf('--version')
const version = at >= 0 ? args[at + 1] : '7.1'
const run = (cmd, argv, cwd) => execFileSync(cmd, argv, { cwd, stdio: 'inherit' })

const work = mkdtempSync(join(tmpdir(), 'ffmpeg-'))
const source = join(work, `ffmpeg-${version}`)
run('curl', ['-fsSL', '-o', join(work, 'ffmpeg.tar.xz'), `https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz`], work)
run('tar', ['-xf', 'ffmpeg.tar.xz'], work)

const decoders = ['mp3float', 'mp3', 'aac', 'aac_latm', 'alac', 'flac', 'vorbis', 'pcm_s16le', 'pcm_s24le', 'pcm_s32le']
decoders.push('pcm_s16be', 'pcm_s24be', 'pcm_s32be', 'pcm_f32le', 'mjpeg', 'png')
const encoders = ['libmp3lame', 'aac', 'alac', 'flac', 'pcm_s16le', 'pcm_s24le', 'pcm_s16be', 'pcm_s24be', 'mjpeg', 'png']
const demuxers = ['mp3', 'mov', 'flac', 'wav', 'aiff', 'ogg', 'image2']
const muxers = ['mp3', 'ipod', 'flac', 'wav', 'aiff', 'image2']
const list = (flag, names) => names.map((name) => `--enable-${flag}=${name}`)

run(
  './configure',
  [
    '--disable-everything',
    '--disable-autodetect',
    '--disable-doc',
    '--disable-ffplay',
    '--disable-ffprobe',
    '--enable-static',
    '--disable-shared',
    '--enable-libmp3lame',
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
run('make', ['-j4', 'ffmpeg'], source)

const name = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
const built = join(source, name)
if (!existsSync(built)) throw new Error(`ffmpeg was not built at ${built}`)
const out = join(app, 'resources', 'native')
mkdirSync(out, { recursive: true })
copyFileSync(built, join(out, name))
console.log(`[ffmpeg] ${built} -> ${join(out, name)}`)
