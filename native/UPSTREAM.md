# USB sync

Copyright (c) 2026 Chris Le. Licensed under GPL-2.0-or-later. See
`licenses/rbxport/LICENSE` and `licenses/rbxport/COPYING`.

`es-usb` links these crates, so that program is a combined work under the same license.

`es-usb` is the command the app runs. It reads one JSON job and
calls the source-record API `rb_usb::export`.

## Upstream Baseline

The imported sources are rbxport 1.9.0 at
`7fb4dad3f88a2cc0cecec0e718b4dde408fd7448`. `upstream-sources.json` records the source,
runtime crates, test-only support crates and downstream differences. The old
snapshot's exact upstream revision is unknown; its workspace version was not
reliable provenance.

Imported fixtures and tests are retained. Extracted source locations are recorded
in `fileRelocations`; deliberate edits are recorded with upstream and local
SHA-256 values in `derivedFiles`.
`rbl-db`, `rbl-index`, `rbl-analysis` and `rbl-devices` support upstream tests and
examples; they are not dependencies of the shipped helper. They are not a second
application database implementation to adopt during SDK extraction.

The exporter preserves Export Studio's
`.exportstudio-publication` journal and `exportstudio` manifest/recovery paths,
including their tests. This keeps existing USB ownership and recovery records
usable without adopting rbxport's separate manifest. The manifest remains version
2; missing receipts cause reinspection rather than trusted reuse.

All imported crates now live in `native/crates` beside the application crates.
Their original names and version 1.9.0 are retained. Each imported manifest pins
its version explicitly because the native workspace's application version is
0.1.0. Dependency and lint inheritance is provided by `native/Cargo.toml`.
`native/Cargo.lock` is the single lockfile; `es-device` uses sibling format crates.
The helper and addon build from the same workspace.

## SDK Extraction

`rb-model` owns pure primitives and source records. `rb-storage` owns the unchanged
durable-publication implementation formerly in `rbl-core`. Existing format and
engine callers use compatibility re-exports from `rbl-core`. The source audit
verifies the moved durability file against its original upstream path.

The local database, device and analysis-file implementations now live in
`rb-library`, `rb-device` and `rb-anlz`; the former `es-db`, `es-device` and
`es-anlz` crates only re-export them. Snapshot profiling belongs to `es-engine`,
not the SDK. Library snapshot and guarded live APIs take explicit database paths;
installed-library discovery remains a separate convenience module.

`rb-usb` owns source-to-engine preparation and an application-independent export
API. Its public input does not expose internal device identities, manifests,
fallback tracks or preparation errors. `es-usb` only validates the application
job envelope and emits JSON events. The export engine no longer searches the
host for settings: `rb-usb::Options` supplies the directory explicitly, and
settings are staged in the recoverable publication instead of copied afterward.

`rb-anlz` and the upstream-derived `rbl-anlz` codec remain distinct implementations.
Cross-parser tests check generated grids and unknown-section retention; that is
not a claim of full equivalence or justification for deleting either codec.

To audit a refresh, check out the recorded upstream commit separately and run:

```sh
node native/check-upstream.mjs /path/to/rbxport
```

The checker compares unchanged imported files against that checkout, applying
the documented namespace mapping and explicit manifest versions. Moved files are
checked at their recorded paths; the three deliberately derived files require
their recorded upstream and local checksums. It also verifies the retained
license texts. Refresh mapped components deliberately, retain
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
is unavailable; their fixture jobs use schema version 2. `check-layers.mjs` runs
as part of that gate: SDK crates cannot depend on app crates at runtime, the
model has no I/O dependencies, and USB export cannot depend on master-library
implementations. Fixtures,
filesystem checks, emulator results and physical-device compatibility are separate
evidence; this refresh does not certify hardware or run private upstream suites.

The next phase generalizes conflict inspection and decision handling before
extracting the independently maintained `es-rekordbox-sdk`. Copyright and GPL
obligations continue to apply after moves and refactors.
