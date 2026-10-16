# Rust Engine Architecture

Status: target design for the Rust engine refactor. This document describes the intended boundaries; it does not imply that every boundary is already implemented.

## Purpose

The Rust port is intended to improve more than implementation language. It should make persistence rules, application workflows, file formats, and desktop integration independently understandable and testable.

The current workspace already has useful domain crates and use-case modules. The main refactoring target is the orchestration around them: `es-node` currently owns library state, command dispatch, backups, device access, event delivery, and workflow coordination, while the Electron `Library` also routes commands and owns library-related timers.

The refactor has since moved library session ownership, backup policy, and library workflows into `es-engine::LibraryService`. N-API serializes typed service results directly to JavaScript values; Electron retains a compatibility command map, now typed by the shared preload command/parameter contract. USB publication remains a subprocess, behind a versioned job contract and a `UsbPublisher` port.

## Design Goals

- Give each policy and resource one authoritative owner.
- Keep Rekordbox database rules isolated from application workflows.
- Keep N-API and Electron boundaries as adapters, not business-logic layers.
- Make commands, results, errors, and progress events explicit and typed within Rust.
- Preserve current behavior during migration, especially transaction stamping, XML bookkeeping, backups, device safety, and file-cleanup behavior.
- Make workflows testable without Electron, N-API, installed Rekordbox data, or physical USB devices.

## Non-Goals

- Do not move window management, dialogs, menus, settings UI, or OS integration into Rust.
- Do not rewrite every existing use case to achieve uniformity. Retain sound modules and improve their boundaries incrementally.
- Do not remove the USB exporter subprocess solely for architectural purity. Keep it if its isolation is useful, but give that process boundary an explicit contract and owner.
- Do not optimize concurrency before the single-writer and snapshot consistency rules are explicit and tested.

## Target Dependency Shape

```text
Renderer
  -> preload / typed IPC
    -> Electron main adapter
      -> es-node (N-API adapter)
        -> es-engine (application service and use cases)
          -> es-db       Rekordbox database access and writes
          -> es-index    read models and query projections
          -> es-device   device discovery, parsing, and device models
          -> es-backup   backup storage and archive operations
          -> es-tags     audio metadata format operations
          -> es-anlz     Rekordbox analysis-file format operations
          -> es-decode / es-analyse / es-wave / es-deck as needed
```

Dependencies point inward toward domain and storage capabilities. Data and format crates do not depend on `es-engine`, `es-node`, Electron, or renderer types. Avoid cycles; application workflows compose lower-level capabilities.

## Ownership Boundaries

| Owner | Owns | Must not own |
| --- | --- | --- |
| Renderer | UI state, user intent, display formatting | Database, file-format, and device rules |
| Preload / Electron IPC | Narrow transport and serialization to renderer | Business policies or a second command registry |
| Electron main | Window/process lifecycle, dialogs, OS integration, timer scheduling, native-module loading | Library session state, database transactions, backup policy decisions, workflow implementation |
| `es-node` | N-API type conversion, async task setup, callback adaptation, Rust error conversion | Command routing policy, SQL, backup decisions, use-case sequencing, device-cache policy |
| `es-engine` | Library service/session, application workflows, validation and policy, workflow events | SQL schema details, N-API/Electron types, low-level file-format parsing |
| `es-db` | Rekordbox connection, snapshot/live modes, schema queries, transactions, USN/XML bookkeeping | UI policy, filesystem import workflow, N-API serialization |
| `es-index` | Read-only projections and view queries | Mutations or transaction ownership |
| `es-device` | Device formats, parsing, selection metadata, volume primitives | Library session, UI events, library mutation workflows |
| `es-backup` | Backup archive format, storage, verification primitives | When the app should back up or whether an edit is allowed |
| `es-tags` / `es-anlz` | Tag and analysis-file read/write formats | Multi-resource workflow policy |

The Rust application service owns the open library session and decides when a workflow needs a snapshot, guarded live access, backup, or refresh. Electron may schedule a periodic service call, but the decision to perform a daily or pre-edit backup belongs to one Rust policy owner.

## Resource and Mutation Rules

### Library Session

- A service instance represents one selected library and owns its source path, read snapshot, writable state, and session-scoped caches.
- Snapshot reads do not mutate the source database.
- A write workflow checks the writable policy and Rekordbox process guard, opens live access for the duration of the operation, commits through `es-db`, and refreshes the snapshot before returning data that reflects the write.
- A failed operation must not leave the service claiming a refreshed state when the snapshot refresh failed. Define and test the recovery behavior for a committed write followed by refresh failure.
- Switching or restoring a library invalidates session caches and produces one well-defined reload event.

### Database Writes

- `es-db` is the only crate that issues SQL against `master.db` or owns a transaction.
- Each logical database edit is one transaction through `Tx`; USN counters, row stamps, dependent-row handling, and playlist XML updates stay inside that boundary.
- Use cases express intent through typed `es-db` operations; they do not compose ad hoc SQL in `es-engine` or `es-node`.
- Filesystem changes that cannot share the database transaction must have an explicit order and recovery strategy. Prefer staging and atomic rename where practical; otherwise report partial outcomes clearly and test them.

### Files, Devices, and Backups

- Use cases may coordinate database and file operations, but format-specific behavior stays in its owning crate.
- `es-device` parses device data and exposes device models; `es-engine` decides what importing or syncing those models means for the library.
- `es-backup` performs archive operations; `es-engine` decides when a backup is required and how backup progress affects a workflow.
- USB publication remains behind an explicit publisher interface. The current subprocess can be its implementation. Its job/result schema, versioning, validation, progress, and failure semantics must be documented and tested.

## Application Service and API

`es-engine` should expose a service-oriented API rather than a string dispatcher. Keep workflow implementations in focused modules such as `import`, `analysis`, `convert`, `relocate`, `device_import`, and `sync`; the service composes them and owns session policy without becoming a new god object.

Preferred internal API characteristics:

- Typed request and result structs/enums; no `serde_json::Value` or JSON text as the normal Rust-to-Rust contract.
- Typed error categories that preserve actionable causes while allowing the N-API adapter to produce user-facing messages.
- Typed progress/event enums emitted by use cases and adapted at the boundary.
- Explicit bounds and validation at the service boundary. Do not silently coerce malformed IDs, numbers, or options into defaults unless that behavior is an intentional compatibility rule.
- No transport method names such as `"track.update"` inside application logic.

The renderer may continue receiving its existing JSON-shaped API during migration. Keep that compatibility mapping at the Electron boundary and remove it only when renderer types and tests have moved to the new contract.

## Concurrency

- Treat library writes as single-writer operations.
- Keep the live database handle scoped to a write operation; never share it with independent background work.
- The current implementation serializes service calls behind one N-API-owned mutex. This is a deliberate correctness boundary around one mutable session, not a claim that all workflows are short or contention-free.
- Do not relax that lock based only on the fact that work runs through `AsyncTask`. First measure command latency/queue wait and define a user-visible budget.
- If measurements justify concurrency, keep live writes single-writer and move slow work out of the lock only where the workflow can operate on owned data or an independent snapshot. Specify refresh/cache invalidation and test read/write races before enabling concurrent reads.
- Progress callbacks must not re-enter the service while a write lock is held.

## Proposed Module Placement

```text
native/crates/es-engine/src/
  service.rs          session state, service API, shared policy
  command.rs          typed service requests/results, if a command enum is useful
  event.rs            typed workflow events
  import.rs           file import use case
  analysis.rs         analysis application workflow
  convert.rs          conversion workflow
  device_import.rs    device-to-library import workflow
  sync.rs             library-to-device workflow and publisher port
  ...                 existing focused use cases

native/crates/es-node/src/
  library.rs          N-API adapters calling typed service methods
  usb.rs              N-API adapters for device-facing service methods
  ...                 audio bindings remain separately owned
```

Names and placement may change to match the code as it evolves. The invariant is that the adapter does not decide workflow policy.

## Migration Sequence

### Phase 0: Freeze behavior and record boundaries

- Record current command behavior, event payloads, error categories, and important side effects.
- Add or identify fixture-backed tests for library writes, import, analysis-file writes, backup policy, and USB job generation.
- Keep real Rekordbox testing on a copied library only; never use the installed library as a test target.

**Exit:** behavior-critical paths have repeatable tests or a written manual check before code moves.

### Phase 1: Establish typed service contracts

- Define typed request/result/error/event types for one vertical slice, starting with read-only library operations.
- Add the service surface while retaining existing N-API and renderer contracts as adapters.
- Add serialization and contract tests at the N-API boundary.

**Exit:** one slice reaches the service without string method dispatch or JSON-text parsing inside Rust business logic.

### Phase 2: Move session and policy ownership

- Move open/reload state, writable policy, backup-before-edit decisions, backup scheduling decisions, and session cache ownership from `es-node` into the Rust service.
- Leave timer scheduling in Electron; have it invoke typed service operations.
- Ensure exactly one owner produces library lifecycle events.

**Exit:** N-API owns no library workflow state, and Electron owns no duplicate library policy.

### Phase 3: Migrate workflows by vertical slice

For each slice, move orchestration into the service, preserve renderer behavior through adapters, and add workflow tests:

1. Reads and library lifecycle.
2. Playlist, tag, and cue writes.
3. Track metadata edits and file tag writes.
4. File import, relocation, and missing-file inspection.
5. Analysis, beatgrid application, and conversion.
6. USB import and sync.
7. Backup restore and recovery behavior.

**Exit:** the operation dispatcher contains only protocol adaptation and result serialization; workflow policy and sequencing live in the service.

### Phase 4: Make external boundaries explicit

- Define the USB publisher interface and versioned subprocess contract, or deliberately replace the subprocess behind that interface.
- Document partial-failure handling where a database write and filesystem write cannot be atomic.
- Add compatibility tests for persisted formats and exporter job/result schemas.

**Exit:** process boundaries have named owners, contracts, and failure semantics.

### Phase 5: Keep concurrency evidence-driven

- Retain serialized service access until profiling demonstrates a user-visible contention problem.
- Record a baseline for queue wait and operation duration before changing lock scope.
- If concurrency is warranted, separate slow file/device work from short-lived live DB transactions where correctness permits, then test snapshot refresh and write races.

**Exit:** the serialization policy and the evidence required to change it are explicit. Concurrent reads are a follow-up only when measurements justify their added consistency complexity.

## Architecture Review Checklist

For each new module or refactor, ask:

- Does this layer make a business decision, or merely adapt a value between APIs?
- Is this resource or policy already owned elsewhere?
- Does any code outside `es-db` issue Rekordbox SQL or manage a transaction?
- Does a lower-level crate depend on an application, UI, N-API, or transport type?
- Are errors and partial outcomes explicit at the boundary?
- Can this workflow be tested with fixtures without Electron or installed Rekordbox data?
- Are filesystem side effects ordered and recoverable if a later step fails?
- Is concurrency safe by construction and backed by a test?

## Completion Criteria

The redesign is complete when:

- `es-node` is a thin N-API adapter and does not implement business workflows.
- Electron main owns desktop lifecycle and scheduling, but does not duplicate library command or backup policy.
- `es-engine` owns library-session policy and application workflows through typed APIs.
- Database access, projections, device parsing, backups, and file formats have single, enforced owners.
- The service concurrency policy is explicit, and any expansion beyond serialized access is backed by measurements and race tests.
- Rust unit/integration tests cover service workflows and `es-db` invariants; boundary tests cover N-API and Electron adapters.
- Real-library smoke checks are documented and run against disposable copies, including opening Rust-written copies in Rekordbox.