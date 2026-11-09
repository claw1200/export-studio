import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

// keymap reads the platform as it loads.
;(globalThis as { window?: unknown }).window = { api: { platform: 'darwin' } }

const { withShift, withoutShift } = await import('../src/renderer/src/lib/keymap.ts')
const { deckKeys, playOnlyKeys } = await import('../src/renderer/src/lib/deck/keys.ts')

describe('withShift / withoutShift', () => {
  it('adds Shift just before the key, after the other modifiers', () => {
    assert.equal(withShift('ArrowLeft'), 'Shift+ArrowLeft')
    assert.equal(withShift('Mod+Alt+KeyZ'), 'Mod+Alt+Shift+KeyZ')
    assert.equal(withShift('Shift+ArrowLeft'), null)
  })

  it('takes Shift off, keeping the rest', () => {
    assert.equal(withoutShift('Shift+KeyC'), 'KeyC')
    assert.equal(withoutShift('Mod+Shift+KeyZ'), 'Mod+KeyZ')
    assert.equal(withoutShift('KeyC'), null)
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

  it('takes the play/pause it is given', () => {
    const { player, calls } = fakePlayer()
    let played = false
    ;(playOnlyKeys(player, () => (played = true))['deck.playPause'] as () => void)()
    assert.equal(played, true)
    assert.deepEqual(calls, [])
  })
})
