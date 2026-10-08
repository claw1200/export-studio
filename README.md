# Rekordbox Library Manager
## Overview - The Export Mode You Actually Want

The goal of this app is to provide a more comprehensive set of features for managing a Rekordbox library.

## Layout

- `app/`: the Electron and Svelte app.
- `engine/`: the Python library engine, which reads and writes the rekordbox database over JSON-RPC.
- `native/`: the Rust audio engine for deck playback, scrub, loops and waveform analysis. See [native/README.md](native/README.md).

## Development

Building the native addon needs a Rust toolchain.

```sh
cd app
npm install
npm run dev            # builds the native addon, then starts electron-vite
npm run test:native    # Rust engine tests
npm run typecheck
```
