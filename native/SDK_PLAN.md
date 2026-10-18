# es-rekordbox-sdk

Export Studio will be the first consumer of an independently maintained, generic
Rekordbox SDK. Initially all crates remain in `native`; eventual extraction targets
one `es-rekordbox-sdk` repository containing multiple crates. Registry/package names
will be decided separately. No repository is created by the initial refresh.

## Phase 1 Status

Implemented on 2026-10-17 against rbxport commit
`7fb4dad3f88a2cc0cecec0e718b4dde408fd7448`. The source audit passes with only the
documented namespace substitutions. The combined native/exporter test runner
passes 1,231 tests (five ignored), including real-helper readback, legacy manifest
continuity and playlist revision handling. Application typechecking and production
build pass; the release native addon and helper both rebuild successfully.

Validation limits: application-wide ESLint has pre-existing errors outside the
new runner; the new runner passes lint. Strict Clippy on the exporter is blocked
by upstream documentation warnings and an inherited unknown lint on the local
toolchain. Windows CI is configured but has not been executed locally. Physical
devices and private firmware suites are unverified.

## Phase 2 Status

Completed on 2026-10-17. All ten imported crates and `es-usb`, including tests and
assets, now live in `native/crates`. The 23-member workspace uses one Cargo
manifest/lockfile; imported packages remain 1.9.0 and application packages remain
0.1.0. The old vendor workspace files are removed. Build scripts, CI caches,
application helper discovery and test fixtures use the native build paths.

Copyright and full license texts are retained in `licenses/rbxport`.
`upstream-sources.json` maps the original and current locations; `check-upstream.mjs`
verifies all imported files with only the recorded namespace substitutions and
explicit version declarations. No format algorithms or public API were rewritten.
The existing local and upstream ANLZ implementations remain separate pending the
layering phase's equivalence tests.

Validation: 1,231 tests pass (five ignored), including device-reader and import
fixtures that now require the current schema-v2 helper rather than quietly
skipping. The source/license audit, Cargo graph assertions, application typecheck
and production build, and combined release native build pass. Windows and physical
hardware remain unverified; the existing workspace-wide lint limitations noted
above are unchanged.

## Phase 3 Status

Implemented on 2026-10-18. The SDK has six application-independent entry packages:

| Package | Responsibility |
| --- | --- |
| `rb-model` | Shared values and caller-supplied source records; only serde dependencies |
| `rb-storage` | Existing durable publication and recovery implementation |
| `rb-anlz` | Analysis-file building, decoding and editing |
| `rb-library` | Snapshot reads and guarded master.db writes; explicit paths |
| `rb-device` | USB/volume discovery, paths and device-library reads |
| `rb-usb` | Source-record preparation and recoverable export, progress and results |

The proven `rbl-pdb`, `rbl-onelibrary`, `rbl-anlz`, audio and export engine remain
lower-level implementation crates. Three `es-*` compatibility crates and core
re-exports keep existing application and engine imports working without duplicate
implementations. UI projections, session policy, playback/analysis workflows and
profiling stay outside the SDK. `rb-library::Library::open` uses the write guard
by default; explicit snapshot access and the existing fixture APIs are retained.

The USB service does not accept a database/session or expose preparation state.
Settings paths are explicit and settings are published with the recoverable
transaction. The schema-v2 helper is a JSON transport over that service. Existing
on-device manifest and journal names remain unchanged for media compatibility.

Validation: 1,236 tests pass (five ignored), including direct Rust USB use without
an application session, settings/cancellation regressions, guarded library/device
tests and ANLZ cross-parser coverage. Dependency gates enforce no SDK-to-app runtime
edges, a pure model, and independent USB/master-library access. Application and
release native builds pass. Provenance records unchanged moves and intentional
derived edits separately. Hardware/Windows coverage and existing workspace-wide
lint limits remain as above. Phase 4 has not started.

Codec consolidation, a complete conflict taxonomy and externally published/stable
crate APIs are not claimed by this extraction. Standalone repository extraction
and a CLI/example remain Phase 5 work.

## Phases

1. Refresh to pinned rbxport 1.9.0, preserve existing USB state names, adopt upstream
   sync semantics and validate the real app/helper integration. See
   `upstream-sources.json` and `UPSTREAM.md` for provenance and test commands.
2. Move imported implementations, assets, tests and the helper into the native
   workspace without behavioural changes. Update packaging and use one lockfile.
   Keep mechanical moves separate from renaming. Consolidate overlapping ANLZ
   implementations only after equivalence tests pass.
3. Extract generic models/formats/storage, guarded master-library access, device
   access and USB orchestration. Application state, UI projections, profiling,
   playback and analysis workflows remain outside the SDK. Core APIs take explicit
   paths/options; the USB engine accepts source records, not an application session.
4. Introduce typed inspection/planning/execution/recovery contracts covering
   deletions, metadata, playlist membership/order/hierarchy, cues/grids, missing
   assets and cross-format disagreement. Distinguish warnings, supported decisions
   and blockers; never add a universal integrity/ownership bypass. Revalidate plans
   at execution and publish decision receipts with the recoverable transaction.
5. Verify a minimal independent CLI/example and standalone SDK workspace build.
   Prepare extraction, API documentation, versioning and maintenance gates before
   publishing or creating the separate repository.

Each phase must pass independently before the next starts. The baseline refresh
does not reintroduce the closed deletion-only review PR.

## Maintenance Contract

Retain tested implementations rather than rewriting format compatibility. Preserve
source commit/path mappings, copyright notices, full applicable license texts and
GPL corresponding-source obligations. Attribution alone does not replace those
obligations, and renamed packages are not relicensed.

Once implementation diverges structurally, port upstream fixes selectively with
regression tests and attribution. Keep semantic comparison fixtures and enforce
dependency direction: SDK crates cannot depend on Export Studio application crates,
Electron, NAPI, application resource paths or UI state. Upstream private firmware
tests do not establish compatibility of the derived SDK.