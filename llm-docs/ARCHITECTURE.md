# Architecture and ownership

One binary crate; no Cargo workspace, backend or application framework. All source paths below are relative to the repository root.

## Production tree

```text
src/
  main.rs                    dependency construction, Kit/assets/window initialization
  runtime.rs                 Tokio/GPUI execution substitution and cancel-on-drop Work
  theme.rs                   product theme policy
  catalog/
    mod.rs                   non-rendering feature owner; intentions, state reads, opaque results
    state.rs                 pure loading/selection/generation transitions
  connectors/
    mod.rs                   provider declarations
    catalog/
      mod.rs                 Client, capability-specific Adapter seam and exports
      types.rs               record data used by callers
      error.rs               typed failures
      memory.rs              safe deterministic adapter
      http.rs                loopback transport, private collection DTO, validation
  storage/
    mod.rs                   serialized preference writer and explicit replies
    preferences.rs           nonsensitive format, namespace, path and file mechanics
  views/
    mod.rs
    app_shell.rs             stable window host; one opaque update consumer
    catalog/
      mod.rs                 screen construction/composition
      record_list.rs         record selection rendering
      record_detail.rs       details, loading/empty/error/storage feedback
      search.rs              local form edits/focus, search/reload intentions
  test_support/              test-only controlled catalog and loopback HTTP fixtures
```

Suites live in owner-local tests/ directories. See [EXTENDING.md](EXTENDING.md) for the complete placement table and registration recipes.

## M0 capability library (separate from the starter)

`src/lib.rs` compiles the headless feasibility slices without changing `main.rs` or wiring Git into the catalog. It exports focused capabilities, not an application aggregate:

- `connectors/git/{mod,process}.rs`: immutable discovery/status/diff/file/patch operations and Linux owned child mechanics; Git-scoped executor substitution.
- `diff/`: canonical byte patch parsing/selection; conservative whole-file fallbacks.
- `working_tree/`: queued-intent refresh barrier and checked selection dispatch, not panel rendering.
- `repository/`: window-local in-place navigation/generation and submodule parent stack.
- `input/`: pure key/context resolution and approved exception diagnostics.
- `storage/lazygit.rs`: read-only supplied-source config merge/reload/provenance, setting diagnostics and separate fingerprint trust mechanics.
- `connectors/prompts/`: private operation-scoped fake askpass/editor/sequence-editor IPC.
- `views/{commit_controls,process_probe}.rs`: actual Kit input and injected non-rendering operation-lifetime probes, no transport construction.
- `test_support/git.rs`: test-only isolated temporary Git fixtures shared by owner suites.
- `spikes/templates/`: separate standard-library Go feasibility oracle, not an application dependency.

Suites are declared by their owning production modules and remain in owner-local `tests/` directories. [M0-RESULTS.md](M0-RESULTS.md) defines tested interfaces, shutdown/gate handoff and limitations. In particular the synchronous prototype operation Drop and ungated connector writes must not be shipped as the M1 GUI lifecycle/mutation policy.

## Interfaces and dependency direction

```text
main -> execution + connector + storage + shell constructor
shell -> retains Entity<Catalog>, Entity<CatalogView>, delivery Task
views -> Entity<Catalog> intention/read interface + private children
Catalog -> pure State + typed Client + Execution + optional Persistence
connectors/storage/runtime -X-> views or feature owners
pure State -X-> GPUI or I/O execution
```

The example uses a non-rendering `Entity<Catalog>`. GPUI owns entity storage, while the retained shell governs logical workflow lifetime. Views observe that entity with independent subscriptions; every intention/completion updates the owner then calls `cx.notify()`. State is readable but transition methods stay private to catalog ownership. A view never directly calls a connector or mutates State.

The catalog-specific Adapter has memory and HTTP implementations plus controlled test outcomes. This is a real substitution seam for one capability, not a trait that all providers must implement. Transport and wire details remain private. Feature-specific user feedback belongs to the feature; typed failure strings in this small example are its deliberately simple presentation policy.

## Lifetimes and delivery

1. Main loads small preferences before entering the GUI loop and constructs the ordered writer and connector. It initializes Kit/assets/theme and opens a window.
2. The shell constructs one catalog owner, begins its initial read and constructs a screen using the existing entity.
3. Search submits a query to the owner. It increments the read generation, replaces/cancels prior work and starts the typed connector operation. Completed outcomes are wrapped in opaque updates.
4. Exactly one shell task receives those updates, calls `Catalog::apply` and notifies the entity. Result receivers are not broadcast subscriptions.
5. State applies only the current generation while loading. Already-queued outcomes cannot overwrite a newer request; cancellation alone is not a correctness gate.
6. Views re-render on entity observation. Recreating a screen does not restart the owner, initial load or update consumer. Query submission belongs to the owner; unsubmitted text, focus and input subscriptions belong to the search view.
7. The shell retains its delivery Task. Dropping the owner drops cancel-on-drop work handles. Drop/recreation tests use this same production code, not a test-only lifecycle.

The root is window-scoped, not process-global. If product workflows must outlive windows or be shared across windows, introduce a retained non-rendering application host with explicit shutdown/task semantics. Do not introduce it just to rename the shell.

## Runtime, storage and transport policy

- Runtime supplies executor/time substitution only. Controlled tests use GPUI's scheduler/clock; real I/O uses a small lazy Tokio runtime.
- Feature reads have a six-second deadline; the HTTP adapter has a five-second transport deadline. No implicit retry, write replay, auth refresh or streaming exists.
- HTTP example configuration binds an immutable literal loopback /records endpoint, uses no proxy/redirect, validates records and reports typed/redacted failures. Production provider configuration needs its own rules.
- An ordered worker serializes config writes. Submission is nonblocking; unavailable/full queue and write failures return explicit outcomes. Canceling a UI waiter does not undo a queued write. Newer save revisions gate feedback.
- Preferences contain only the submitted query. Independent instances must use distinct profiles; a single-process ordered worker is not a multi-process locking protocol. File replacement uses a sibling temporary file then rename, not a crash-durability guarantee.
- The base has no credentials, sessions, streams, server/protocol crates or persistent product cache.

## Tests

In-crate suites live in their owner’s tests/<subject>.rs directory and are declared by that production module under cfg(test) with explicit path where needed. They retain private access; normal visibility is not widened for tests. Helpers are local to one suite, under tests/support for a feature, or src/test_support for cross-feature reuse.

Feature tests cross production coordination/state transitions with controlled requests/time. Connector tests additionally exercise owned loopback HTTP serialization/decoding. View journeys use actual Kit controls, semantic IDs and a Kit Root through the same app_shell::open constructor. Storage uses temporary paths and no user files/providers.
