# USB sync

Copyright (c) 2026 Chris Le. Licensed under GPL-2.0-or-later. See
`licenses/rbxport/LICENSE` and `licenses/rbxport/COPYING`.

`es-usb` links these crates, so that program is a combined work under the same license.

`es-usb` is the command the app runs. It reads one JSON job and
calls `rbl_export::export_cancellable`.

## Upstream Baseline

The imported sources are rbxport 1.9.0 at
`7fb4dad3f88a2cc0cecec0e718b4dde408fd7448`. `upstream-sources.json` records the source,
runtime crates, test-only support crates and downstream differences. The old
snapshot's exact upstream revision is unknown; its workspace version was not
reliable provenance.

All ten imported crate directories retain upstream sources, fixtures and tests.
`rbl-db`, `rbl-index`, `rbl-analysis` and `rbl-devices` support upstream tests and
examples; they are not dependencies of the shipped helper. They are not a second
application database implementation to adopt during SDK extraction.

The implementation changes within those directories preserve Export Studio's
`.exportstudio-publication` journal and `exportstudio` manifest/recovery paths,
including their tests. This keeps existing USB ownership and recovery records
usable without adopting rbxport's separate manifest. The manifest remains version
2; missing receipts cause reinspection rather than trusted reuse.

All imported crates now live in `native/crates` beside the application crates.
Their original names and version 1.9.0 are retained. Each imported manifest pins
its version explicitly because the native workspace's application version is
0.1.0. Dependency and lint inheritance is provided by `native/Cargo.toml`.
`native/Cargo.lock` is the single lockfile; `es-device` uses sibling format crates.
The helper and addon build from the same workspace. No format implementation or
ANLZ consolidation is part of this mechanical move.

To audit a refresh, check out the recorded upstream commit separately and run:

```sh
node native/check-upstream.mjs /path/to/rbxport
```

The checker compares every imported file against that checkout, applying only
the documented namespace mapping and explicit manifest versions. It also verifies
the retained license texts. Refresh all imported crates together, retain
their test assets and licenses, and update the recorded commit only after the
full test gate passes. Do not copy upstream files over application-owned `es-usb`.

## Job and Sync Contract

`es-usb` accepts schema version 2 only. Version 1 jobs and the obsolete `overwrite`
field are rejected before recovery or export. Rebuild the helper and native addon
together. Library cues and beat grids replace device edits during sync; when the
library has no analysis, usable device analysis is retained. Library playlist
membership timestamps are supplied for upstream's device-addition reconciliation.

## Validation

From `app`, run `npm run test:native`. It builds the actual helper before testing
the native workspace, selects that helper explicitly, and enables installed-library
write protection. Sync, device-reader and device-import tests fail if the helper
is unavailable; their fixture jobs use schema version 2. Fixtures,
filesystem checks, emulator results and physical-device compatibility are separate
evidence; this refresh does not certify hardware or run private upstream suites.

The next phase separates the generic SDK API from application workflows before
extracting the independently maintained `es-rekordbox-sdk`. Copyright and GPL
obligations continue to apply after moves and refactors.
