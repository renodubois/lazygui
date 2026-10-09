# Extending LazyGUI

Read [ARCHITECTURE.md](ARCHITECTURE.md), [M1-RESULTS.md](M1-RESULTS.md) and [VERIFY.md](VERIFY.md) first. Extend the installed Git slice, not the removed catalog example. Future paths below are recipes, not empty scaffolds to create.

## Placement

| Change | Owner / wiring |
| --- | --- |
| Repository readiness/switch/config policy | `src/repository/`; constructed by startup, retained in shell |
| Files/index/staging behavior | `src/working_tree/`; intent/read interface used by repository view |
| Commit draft/message/warning/results | `src/commit/`; native editable controls in `src/views/commit_controls.rs` |
| Read-only HEAD/history | `src/history/`; shell retains owner and delivery |
| Canonical patch transform | `src/diff/`; pure bytes/IDs, no views/Git I/O |
| New local workflow (refs/stash/etc.) | Focused `src/<feature>/`; library declaration, startup injection, shell retention only when implemented |
| Typed Git operation/codec | `src/connectors/git/`; requested by its feature, not by controls |
| Provider-specific external transport | Future `src/connectors/<provider>/`; own typed capability/seam, no universal connector |
| Config/file mechanics | `src/storage/`; workflow decides use/persistence |
| Screen/private panel/dialog | `src/views/<owner>/`; parent composition and local focus/interaction |
| Scoped action/help/click metadata | `src/views/repository/actions.rs` or owning control's definitions; not a universal executor |
| Theme | `src/theme.rs` |
| Suite | Owning module's `tests/<subject>.rs`, explicit `cfg(test)`/path declaration |
| Shared fixture | Owner's `tests/support/`; cross-feature test-only `src/test_support/` |

`src/lib.rs` exports owners/capabilities with path declarations; `src/main.rs` declares binary theme/views. Shell suites belong under `src/views/app_shell/tests/` and are registered by `app_shell/mod.rs`. Root `tests/` is only for public-library integration crates. No inline suites, sibling `*_tests.rs` or production visibility widening.

## Add a connected workflow

1. Define a focused intent/read/result contract and authoritative owner. Views retain focus, unsubmitted local control edits, filters/scroll; owners retain business drafts/data and mutation decisions.
2. Construct dependencies in startup. Reuse the existing shared process host, mutation gates and ordered writer; never make independent per-window gates/writers.
3. Retain the owner above replaceable screens, with exactly one opaque update consumer. Shell delivers `apply` and `cx.notify()`; it must not interpret every feature result.
4. Use owner identity tokens and generations to reject stale/cross-owner outcomes. A screen recreated around an existing owner must not start another initial read/delivery.
5. For mutations, synchronously admit a retained workflow **before spawning**. Acquire the correct Worktree/Shared/All lease on a worker. Verify canonical identity/targets, dispatch explicit argv, settle child work, reconcile authoritative state even on error/cancel, then release the lease at the owner-defined reconciliation boundary.
6. Keep workflow retention through gate waits, between commands, cleanup and opaque completion/drop. Shutdown rejects new workflows while admitted cleanup reads remain possible. UI Drop never joins/waits on child workers.
7. Bind byte paths with literal pathspec semantics and `--`; display/clipboard text is not an operation target. Alternate indices are currently unsupported; adding them needs explicit identity/gate design.
8. Do not automatically replay an uncertain write. Record command-specific cancellation/confirmation and observed state. Hooks/filters can have side effects even when Git exits unsuccessfully.
9. Add installed-Git disposable fixtures and controlled error/out-of-order/cancel/close tests through production coordination. Test actual index/worktree/message bytes, not only labels.
10. Update the matrix/setting diagnostics and run `./scripts/check.sh`. Native acceptance is separate.

## Extend views/input

Use actual Kit controls and stable semantic IDs/accessibility labels/test registration. Accept only focused owner handles/data. Popups/dialogs stay private to their owning view; no product-wide dialog registry.

Resolve all contextual actions before universal actions, including contextual overrides of universal names. Use the same scoped definitions for handlers, help and clickable labels; remove shadowed shortcut labels. Menus/text prompts/editors suppress underlying repository commands. Check marked text **before** toolkit actions can clear it; leave IME/caret/selection/paste to the native controls. Body Enter is newline, subject Enter submits, configured confirmation/menu keys remain distinct. Ctrl+O is commit options only in commit editing, copy in repository context.

Canonical range anchors include path/side/raw snapshot, not wrapped row offsets. Refresh remaps surviving IDs or clears consumed/ambiguous anchors. Hidden/collapsed/filtered file targets cannot be staged through a retained range. Validate hunk/line/sticky/shift ranges and rapid interleaved keys on both sides.

## Extend configuration/storage

Add a real consumer before classifying a setting/binding as supported. `src/storage/lazygit/supported.rs` deliberately separates **supported**, **parsed unavailable**, and unknown/unsupported fields. Full LazyGit config parity is not established. Validate the entire reload candidate before replacing settings; keep source-specific diagnostics.

Shared YAML must remain read-only and global relative source paths frozen across switches/reloads. Startup anchors globals against invocation cwd before processing `--path`. GUI preferences/trust stay in their own namespace. Ordered storage supports writes, but current startup only reads window size/trust and shell flushes; saving resize/recent paths and approval UI require deliberate integration and tests. Replies are explicit; dropping a waiter does not undo an accepted write. No credentials/drafts/command definitions in profile files.

Branch prefix rules use a conservative Go-compatible regex subset and Go-style replacement expansion, not templates. Unsupported/invalid constructs transactionally fail; do not silently substitute broader Rust semantics. Preserve repository-first/global fallback order and empty first-match behavior. New-draft preparation must never overwrite edited/recalled/cancelled drafts or re-prefix retries.

`customCommands`/templates remain unavailable; the separate Go feasibility oracle/`templatesGo` route is **not an approved production helper/dependency**. Approval mechanics alone cannot enable execution. Before future execution, guard templates/suggestions/menus/final commands by source fingerprint trust. Network auto-fetch belongs to M3, not refresh.

## Add another external provider

First define the actual capability and immutable account/endpoint context. Keep transport, private DTO decoding, bounds/deadlines/redaction in its connector; workflow generations/retries/uncertainty belong to its feature. Inject a capability-specific fake seam and test real protocol on disposable loopback listeners only. Do not copy removed demo HTTP validation or sample deadlines as product policy.

Credential/prompt/tool integration needs its own security/lifetime design. The M0 prompt bridge is fake-only, not shipped askpass/editor/signing/SSH support. Real providers/keyrings/native launches require separate consent and isolated sessions; XDG isolation does not isolate a wallet.
