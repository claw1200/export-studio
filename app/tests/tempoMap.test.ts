import { register } from 'node:module'
import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

register(new URL('./resolve-ts.mjs', import.meta.url).href)

const tm = await import('../src/renderer/src/lib/tempo-map.ts')
type TempoMap = import('../src/renderer/src/lib/tempo-map.ts').TempoMap
type TempoFlag = import('../src/renderer/src/lib/tempo-map.ts').TempoFlag

const DURATION = 240_000

function flag(startMs: number, bpm: number, extra: Partial<TempoFlag> = {}): TempoFlag {
  return {
    id: tm.newFlagId(),
    startMs,
    bpm,
    rampBeats: 0,
    beatNumber: null,
    loaded: null,
    savedBeat: 0,
    savedBeatsPerBeat: 1,
    ...extra
  }
}

function asSaved(map: TempoMap): { times: number[]; beats: number[]; bpms: number[] } {
  const g = tm.compileTempoMap(map)
  return { times: g.times.map(Math.round), beats: g.beats, bpms: g.bpms.map(tm.roundBpm) }
}

function shape(map: TempoMap): string[] {
  return map.flags.map((f) => `${f.bpm}${f.rampBeats ? `/ramp${f.rampBeats}` : ''}${tm.varies(f) ? '~' : ''}`)
}

const DJ_EDIT: TempoMap = {
  durationMs: DURATION,
  flags: [
    flag(412, 136, { beatNumber: 1 }),
    flag(412 + 128 * (60_000 / 136), 150, { rampBeats: 32 }),
    flag(110_000, 174, { beatNumber: 1 }),
    flag(180_000, 140)
  ]
}

describe('tempo flags', () => {
  it('compiles a ramp as one tempo step per beat', () => {
    const g = tm.compileTempoMap(DJ_EDIT)
    const start = g.sectionStart[1]
    assert.equal(g.bpms[start - 1], 136)
    assert.ok(Math.abs(g.bpms[start] - (136 + 14 / 32)) < 1e-9)
    assert.equal(g.bpms[start + 31], 150)
    assert.equal(g.bpms[start + 32], 150)
  })

  it('reads a saved grid back as the same flags, ramps included', () => {
    const back = tm.tempoMapOf(asSaved(DJ_EDIT), DURATION)!
    assert.deepEqual(shape(back), ['136', '150/ramp32', '174', '140'])
    assert.equal(back.flags[2].beatNumber, 1)
  })

  it('saves an unedited grid exactly as it was loaded', () => {
    const first = asSaved(DJ_EDIT)
    const again = asSaved(tm.tempoMapOf(first, DURATION)!)
    assert.deepEqual(again, first)
  })

  it('keeps a ramp after an edit elsewhere, and still reads it back as one flag', () => {
    const loaded = tm.tempoMapOf(asSaved(DJ_EDIT), DURATION)!
    const edited = tm.setFlagBpm(loaded, 1, 152)
    assert.deepEqual(shape(tm.tempoMapOf(asSaved(edited), DURATION)!), ['136', '152/ramp32', '174', '140'])
  })

  it('reads a grid whose tempo wobbles beat by beat as one section', () => {
    const times: number[] = []
    const bpms: number[] = []
    const beats: number[] = []
    let t = 300
    for (let i = 0; i < 400; i++) {
      const bpm = Math.round((128 + Math.sin(i * 1.7) * 0.4) * 100) / 100
      times.push(Math.round(t))
      bpms.push(bpm)
      beats.push((i % 4) + 1)
      t += 60_000 / bpm + (i % 3) - 1
    }
    const map = tm.tempoMapOf({ times, beats, bpms }, times[times.length - 1] + 100)!
    assert.equal(map.flags.length, 1)
    assert.ok(tm.varies(map.flags[0]))
    assert.deepEqual(asSaved(map).times, times)
  })

  it('adds a flag without moving any beat', () => {
    const before = asSaved(DJ_EDIT)
    const added = tm.addFlag(DJ_EDIT, 60_000)!
    assert.equal(added.map.flags.length, 5)
    assert.deepEqual(asSaved(added.map), before)
  })

  it('changes one section without moving the flags after it', () => {
    const map = tm.setFlagBpm(DJ_EDIT, 2, 175)
    assert.deepEqual(
      map.flags.map((f) => f.startMs),
      DJ_EDIT.flags.map((f) => f.startMs)
    )
    const shifted = tm.shiftSection(DJ_EDIT, 2, 25)
    assert.equal(shifted.flags[2].startMs, 110_025)
    assert.equal(shifted.flags[3].startMs, 180_000)
  })

  it('stretches a section so a chosen beat lands on the target', () => {
    const map = tm.stretchSection(DJ_EDIT, 3, 16, 180_000 + 16 * (60_000 / 141))
    assert.equal(map.flags[3].bpm, 141)
  })

  it('holds the beat at the playhead when retempoing the first section', () => {
    const g = tm.compileTempoMap(DJ_EDIT)
    const held = g.times[40]
    const map = tm.setSectionBpm(DJ_EDIT, 0, 137, held)
    const after = tm.compileTempoMap(map)
    assert.ok(after.times.some((t: number) => Math.abs(t - held) < 1e-6))
  })

  it('moves cues with the section they sit in', () => {
    const loaded = tm.tempoMapOf(asSaved(DJ_EDIT), DURATION)!
    const grid = tm.compileTempoMap(loaded)
    const savedBeat = grid.sectionStart[2] + 8
    const shifted = tm.shiftSection(loaded, 2, 30)
    const at = tm.timeOfSavedBeat(shifted, tm.compileTempoMap(shifted), savedBeat)
    assert.ok(Math.abs(at - (grid.times[grid.sectionStart[2] + 8] + 30)) < 1e-6)
  })

  it('removes a flag and lets the section before carry on', () => {
    const map = tm.removeFlag(DJ_EDIT, 3)
    assert.deepEqual(shape(map), ['136', '150/ramp32', '174'])
    const g = tm.compileTempoMap(map)
    assert.equal(g.bpms[g.bpms.length - 1], 174)
  })
})
