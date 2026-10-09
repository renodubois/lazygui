# Architecture and ownership

One package with a capability/feature library (`src/lib.rs`) and GPUI binary (`src/main.rs`), no workspace/backend. Startup installs real Git owners; catalog, HTTP adapter and sample runtime/query persistence are removed.

## Actual production map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Byte-preserving CLI, process/gate/storage construction, Kit/assets/theme/window setup |
| `src/lib.rs` | Focused module exports; no all-purpose application aggregate |
| `src/repository/mod.rs` | Readiness/config session, in-place open/switch/reload, owner token/generation; separate pure navigation prototype |
| `src/working_tree/{mod,owner}.rs` | Authoritative status/patch snapshots, file/selection intents, queue, stale verification, post-write cursor/selection/focus reconciliation |
| `src/commit/{mod,message,prefix}.rs` | Draft lifetime/preparation, warning/submit/cancel/retry policy, message rules, branch prefix, observed commit outcomes |
| `src/history/mod.rs` | Retained read-only HEAD and bounded (100 records) history |
| `src/diff/mod.rs` | Pure canonical byte parsing, snapshot-scoped line/hunk/change mapping, subset generation and identity remapping |
| `src/input/mod.rs` | Pure key spelling/context spike and policy diagnostics; not a universal product executor |
| `src/connectors/git/{mod,reads,types}.rs` | Immutable identity-bound argv, byte paths, status/diff/history/object codecs, typed failures |
| `src/connectors/git/gates.rs` | Shared index/worktree/common-directory leases |
| `src/connectors/git/process.rs` | Linux owned non-PTY children/pipes/groups, cancellation, retained workflow admission and shutdown acknowledgments |
| `src/storage/lazygit.rs` | Supplied-field config merge, provenance/diagnostics and source fingerprint trust mechanics |
| `src/storage/lazygit/{discovery,supported,prefix}.rs` | Read-only source discovery, typed supported/parsed subset, conservative Go-compatible prefix regex/replacement semantics |
| `src/storage/lazygit/gui.rs` | Separate profile file mechanics and one ordered writer |
| `src/views/app_shell/mod.rs` | Stable window host retaining repository and independent tree/history/commit entities, one opaque delivery task per owner, refresh/close lifecycle |
| `src/views/repository/{mod,actions}.rs` | Composition, local focus/filter/ranges/popups; scoped definitions for keys/help/click labels and delegation to owners |
| `src/views/commit_controls.rs` | Real Kit input/textarea, local focus/IME/menu/clipboard; emits commit intentions |
| `src/theme.rs` | Kit dark theme |
| `src/connectors/prompts/mod.rs` | Retained M0 fake-only IPC prototype; not wired as production askpass/editor helper |
| `src/views/process_probe.rs` | M0 headless process-lifetime probe; not product shell |
| `src/test_support/git.rs` | Test-only cross-feature isolated Git fixture |

Library path exports compile commit controls/process probe from views and Git/config/prompts from their owning directories. The binary declares theme and repository/shell views. No universal connector trait, event bus, catch-all action executor, global dialog registry or generic app dependency bag.

## Construction and delivery

```text
main -> shared ProcessHost + MutationGates + OrderedStorage + Repository::open
     -> views::app_shell::open -> AppShell -> RepositoryView
accepted repository session -> WorkingTree::new + History::new + Commit::new
RepositoryView / CommitControls -> focused owner intentions/read interfaces
owners -> Git Client + worker scopes; pure diff/message/config vocabulary
shell -> opaque owner Update::apply -> cx.notify()
connectors/storage -X-> views or product coordination
```

Owners are non-rendering GPUI entities retained above replaceable controls. Constructors start readiness/tree/history reads once. Receiver clones are not broadcast; the shell retains exactly one delivery task per owner. Every entity intention/completion notifies observers. Foreign worker channels are polled through the same 16 ms GPUI timer bridge in production and fake-clock tests.

Repository/session clients are immutable and bound to canonical worktree/Git/common-dir identities. Results carry owner tokens and generations; cancellation alone is not a correctness check. Failed config reload retains active session/policies. Accepted reload replaces owners together and preserves the same-repository draft; old overlay callbacks are released, reopening uses new policy. Switching refuses busy/queued tree/commit work and rejects prior results.

## Mutation and lifecycle contract

- One startup-created `MutationGates` is shared across windows. Worktree scope locks canonical Git-dir index identity plus worktree; Shared scope locks common directory; All acquires both atomically. Linked worktrees have independent indices but shared refs. Bare Worktree/All acquisition fails. Git's own locks remain authoritative; external tools can race.
- Low-level connector writes intentionally do **not** acquire gates themselves. Owners hold leases over target verification, write and authoritative reconciliation. Working-tree updates retain the lease through UI target/focus reconciliation; commit retains All through HEAD/status reconciliation.
- Each owner admits a `WorkflowScope` synchronously before spawning. It stays retained across gate waits, between commands, cleanup and queued opaque delivery/drop. Shutdown closes new admission, cancels scopes, but permits already-admitted cleanup reads with fresh cancellation flags.
- `Operation::drop`/owner drops request cancellation without UI-thread waits/joins. Native child cleanup runs on workers: kill owned group, reap child, drain/join bounded pipe workers. Children that daemonize/escape that group are outside the guarantee.
- Shell close drops/cancels relevant owners, flushes the ordered writer and waits asynchronously for acknowledgment. Last-window removal/quit rechecks after concurrent closes/flushes; one window must not shut down another still-open window's host prematurely. Abrupt OS termination/crash is not graceful shutdown.
- No fixed sample read deadline or automatic mutation retry. Errors/cancel may have side effects; commit compares actual HEAD/status and reports committed/not-committed/uncertain. Bounded/redacted feedback is not a claim arbitrary hook output can never contain secrets.

## Targets, refresh and input

Canonical applicable bytes never come from whitespace-filtered display, wrapped rows, lossy path text or clipboard. Snapshot-scoped IDs are remapped using bytes/coordinates after known writes; consumed/ambiguous anchors are cleared, not clamped. Whole-file intentions retain enqueue-time snapshots and both rename paths. They reject external replacement after a pending read; mixed file ranges stage pending members, while entirely staged ranges unstage. Partial new/delete regular text is supported; missing newline, binary, symlink/gitlink, rename/copy/mode and unsafe zero-context/whitespace views have explicit fallback.

The tree owner queues intents and reconciles before dispatching the next. Unmoved repeat keys may select a fresh successor; explicitly navigated/ranged targets retain only surviving identities, never another file's row. Failed/uncertain actions clear queued writes. Automatic polling coalesces behind slow reads/actions; explicit refresh may supersede a read but waits behind a mutation. History skips ticks while busy. Polling is not a watcher or network auto-fetch.

Repository action definitions are view-scoped vocabulary, not cross-feature execution infrastructure. Contextual bindings (including contextual overrides of universal names) precede universal bindings. Help/click labels omit shadowed keys. Commit controls have their own cohesive shortcut definitions: editable and popup contexts are exclusive; marked composition is left to Kit/IME before actions clear it.

## Storage boundary and remaining seams

Read-only shared YAML and GUI profile are distinct. Global relative sources freeze at invocation cwd before CLI `--path` processing; direct owner construction also freezes them before navigation. Supported versus parsed-unavailable setting/binding inventories are in `supported.rs` and visible diagnostics. Prefix compilation is transactional; it accepts only a conservative Go-compatible subset, not all Rust or Go regex syntax.

Startup reads preferences/trust, consumes window size and retains one ordered writer. Shell flushes on close. **No production call saves preferences or approves/revokes trust yet**; recent repositories/resize persistence and approval UI are remaining integration work. Fingerprint mechanics are tested but custom/template execution is unavailable. Atomic file replacement is not durable fsync or cross-process coordination.

## Tests

All in-crate suites live in the owning module's `tests/<subject>.rs`, registered with `cfg(test)`/path; no inline/sibling suites or visibility widening. Shell suites live in `src/views/app_shell/tests/` and are declared by `app_shell/mod.rs`; local view/navigation by repository view; startup parser by `main.rs`. Shell-shared fixtures live in `src/views/app_shell/tests/support/`; cross-feature Git fixtures in test-only `src/test_support/`.

[M1-RESULTS.md](M1-RESULTS.md#automated-acceptance-matrix) links exact installed-Git and headless production-constructor/control journeys. M0 prompt/template feasibility is historical evidence, not shipped integrations. See [VERIFY.md](VERIFY.md) for safety and native acceptance boundaries.
