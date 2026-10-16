# Native audio

For the library engine's layer boundaries and refactoring direction, see [ARCHITECTURE.md](ARCHITECTURE.md).

The deck plays through a Rust engine loaded into Electron's main process as a Node addon. The renderer never touches audio: it sends commands and draws from position ticks.

```
renderer  Deck (deck.svelte.ts)  ──window.audio.send──▶  preload  ──deck:cmd──▶  main/audio/bridge.ts
          clock.ts extrapolates   ◀──────── Tick {seq, decks} at ~30 Hz ─────────  es_node.node
```

## Crates

| Crate | Role |
| --- | --- |
| `es-decode` | Streams any file symphonia reads, resampled with rubato to the device rate. Seeks are frame-accurate. |
| `es-deck` | One decode thread per deck fills a lock-free ring (rtrb) of 512-frame blocks. The cpal callback only copies, fades and mixes. |
| `es-wave` | The waveform scan, ported from the renderer and run in parallel with rayon. Parity tests check it against fixtures the TypeScript scan produced. |
| `es-analyse` | Track analysis for import: tempo, beatgrid (normal or dynamic), downbeats, key, and per-band loudness for the waveform tags. Our own algorithm, not rekordbox's. |
| `es-node` | The napi-rs bindings: `open`, `command`, `snapshot`, `scrub*`, and async `analyse` and `analyseTrack`. |

## How the deck stays tight

- **Generations.** Each transport command bumps the deck's generation. Blocks decoded for an older generation are dropped in the callback, so a seek or cue is heard within one device buffer, not after the ring drains.
- **Fades.** Every start, stop, jump and drop crosses over in 88 frames (2 ms at 44.1 kHz), so nothing clicks.
- **`stopAt`.** Pauses and seeks in a single command, so CUE never plays a stray block from the old spot.
- **Loops.** The decode thread cuts each block at the out point and carries on from the in point. The loop is seamless at any length, and the callback stays loop-unaware.
- **Ticks.** Each tick echoes the highest command sequence the main process applied. The renderer ignores ticks older than its last command, extrapolates between ticks (wrapping inside an active loop), and eases small corrections rather than jumping.
- **Scrub.** While a drag is live the deck switches to a shallow ring of 3 blocks. A read head follows the pointer's speed, measured from the renderer's event timestamps, through a ±2 s PCM window. A speed-tracking Butterworth low-pass takes the edge off slow drags, as on vinyl. Letting go lands exactly where the pointer stopped.

Tempo and pitch are not built yet. They will slot in as a `SetTempo` command handled by the decode thread, and as the `rate` field the renderer's anchor already carries.

## MP3 timing

Rekordbox's timeline runs one MPEG frame later than a plain decode for MP3s whose Xing/Info header carries a `LAME` tag: 1152 samples for MPEG-1 and 576 for MPEG-2. Files without the tag, or encoded with `Lavc`, already line up. `es-decode/src/mp3.rs` detects the tag and the decoder emits that much silence first. With this, the beat grids and the ANLZ waveforms line up to within ±3 ms on a sample of 30 MP3s. Gapless trimming is deliberately left off.

## Building and testing

```sh
cd app
npm run build:native        # release build, copied to app/resources/native/es_node.node
npm run build:native:debug
npm run test:native         # cargo test --workspace
```

`npm run dev` builds the addon first. Release builds bundle it via `electron-builder.yml`.
