import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { describe, it } from 'node:test'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'

function libraryWith(native) {
  const source = readFileSync(new URL('../src/main/library.ts', import.meta.url), 'utf8')
  const compiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText
  const context = {
    exports: {},
    require: (name) => {
      if (name === 'electron') return { app: {} }
      if (name === '@electron-toolkit/utils') return { is: { dev: true } }
      if (name === 'path') return {}
      throw new Error(`Unexpected import: ${name}`)
    },
    setInterval,
    clearInterval,
    console
  }
  runInNewContext(compiled, context)
  return new context.exports.Library(() => ({
    RekordboxLibrary: function () {
      return { onEvent() {}, ...native }
    }
  }))
}

describe('media request dispatch', () => {
  it('forwards an encoded USB artwork request to the native resolver', async () => {
    const calls = []
    const library = libraryWith({
      mediaResolve: async (...args) => {
        calls.push(args)
        return '/Volumes/USB_1/PIONEER/Artwork/00013/a243_m.jpg'
      }
    })
    const id = decodeURIComponent('usb%3A%252FVolumes%252FUSB_1%3A37')
    assert.equal(
      await library.call('media.resolve', { kind: 'art', id, size: 'm' }),
      '/Volumes/USB_1/PIONEER/Artwork/00013/a243_m.jpg'
    )
    assert.deepEqual(calls, [['art', 'usb:%2FVolumes%2FUSB_1:37', 'm']])
  })

  it('keeps USB audio on its existing route', async () => {
    const id = 'usb:%2FVolumes%2FUSB_1:37'
    const library = libraryWith({
      usbMedia: async (key) => {
        assert.equal(key, id)
        return '/Volumes/USB_1/Contents/track.mp3'
      }
    })
    assert.equal(
      await library.call('media.resolve', { kind: 'audio', id }),
      '/Volumes/USB_1/Contents/track.mp3'
    )
  })

  it('keeps desktop artwork on the native resolver', async () => {
    const library = libraryWith({
      mediaResolve: async (...args) => {
        assert.deepEqual(args, ['art', '37', 's'])
        return '/library/cover_s.jpg'
      }
    })
    assert.equal(
      await library.call('media.resolve', { kind: 'art', id: '37', size: 's' }),
      '/library/cover_s.jpg'
    )
  })
})