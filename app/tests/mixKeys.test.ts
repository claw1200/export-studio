import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

// keymap reads the platform as it loads.
;(globalThis as { window?: unknown }).window = { api: { platform: 'darwin' } }

const { withModifier, withoutModifier, shortcutHint } = await import('../src/renderer/src/lib/keymap.ts')
const { deckKeys, playOnlyKeys } = await import('../src/renderer/src/lib/deck/keys.ts')

describe('withModifier / withoutModifier', () => {
  it('adds the modifier in the order chordFromEvent writes it', () => {
    assert.equal(withModifier('ArrowLeft', 'Shift'), 'Shift+ArrowLeft')
    assert.equal(withModifier('Mod+Alt+KeyZ', 'Shift'), 'Mod+Alt+Shift+KeyZ')
    assert.equal(withModifier('Shift+KeyC', 'Alt'), 'Alt+Shift+KeyC')
    assert.equal(withModifier('Mod+KeyC', 'Ctrl'), 'Mod+Ctrl+KeyC')
    assert.equal(withModifier('Shift+ArrowLeft', 'Shift'), null)
  })

  it('takes the modifier off, keeping the rest', () => {
    assert.equal(withoutModifier('Shift+KeyC', 'Shift'), 'KeyC')
    assert.equal(withoutModifier('Mod+Alt+KeyZ', 'Alt'), 'Mod+KeyZ')
    assert.equal(withoutModifier('KeyC', 'Alt'), null)
  })

  it('round-trips every chord', () => {
    for (const modifier of ['Shift', 'Alt', 'Ctrl'] as const)
      for (const chord of ['KeyC', 'Mod+KeyZ', 'Alt+ArrowLeft', 'Shift+Space']) {
        const added = withModifier(chord, modifier)
        if (added) assert.equal(withoutModifier(added, modifier), chord)
      }
  })
})

describe('shortcutHint', () => {
  it('shows the chord with the layer modifier for deck B', () => {
    assert.equal(shortcutHint(['KeyC']), ' (C)')
    assert.equal(shortcutHint(['KeyC'], 'Alt'), ' (⌥C)')
  })
})

/** Records which player methods a handler calls. */
function fakePlayer(): { player: never; calls: string[] } {
  const calls: string[] = []
  const player = new Proxy(
    { position: 0, quantise: false },
    {
      get: (target, key: string) =>
        key in target
          ? target[key as keyof typeof target]
          : (...args: unknown[]) => calls.push(`${key}(${args.join(',')})`)
    }
  )
  return { player: player as never, calls }
}

describe('playOnlyKeys', () => {
  it('leaves out everything that edits the track', () => {
    const { player } = fakePlayer()
    const keys = playOnlyKeys(player)
    for (const id of ['cue.addMemory', 'cue.deleteAtPlayhead', 'deck.gridEdit', 'grid.addFlag', 'grid.undo']) {
      assert.equal(id in keys, false, id)
    }
    assert.ok('cue.addMemory' in deckKeys(player))
  })

  it('has the memory cue and loop length keys', () => {
    const { player, calls } = fakePlayer()
    const keys = playOnlyKeys(player)
    for (const id of [
      'cue.nextMemory',
      'cue.previousMemory',
      'deck.beatLoop',
      'deck.loopHalve',
      'deck.loopDouble'
    ] as const) {
      ;(keys[id] as () => void)()
    }
    assert.deepEqual(calls, [
      'callMemoryCue(1)',
      'callMemoryCue(-1)',
      'beatLoop(4)',
      'resizeLoop(0.5)',
      'resizeLoop(2)'
    ])
  })

  it('calls hot cues without storing new ones', () => {
    const { player, calls } = fakePlayer()
    const hot = playOnlyKeys(player)['cue.hot1'] as { down: () => void }
    hot.down()
    assert.deepEqual(calls, ['hotCueDown(1)'])
    calls.length = 0
    ;(deckKeys(player)['cue.hot1'] as { down: () => void }).down()
    assert.deepEqual(calls, ['hotCueDownOrStore(1)'])
  })

  it('plays and pauses the deck itself', () => {
    const { player, calls } = fakePlayer()
    ;(playOnlyKeys(player)['deck.playPause'] as () => void)()
    assert.deepEqual(calls, ['toggle()'])
  })
})
