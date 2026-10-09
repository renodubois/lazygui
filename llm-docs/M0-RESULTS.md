# M0 — feasibility and contract closure

Status: **complete for the M0 exit gate**, 2026-10-08. This is headless feasibility evidence, not a usable Git client, M1 completion, or native parity. The executable still opens the catalog starter. The capability prototypes are compiled through `src/lib.rs`; they are not secretly wired into startup.

## Verification

- `./scripts/check.sh`: formatting, strict all-target Clippy, **45 Rust tests** (25 M0 + 20 starter), locked build: passed.
- `bash scripts/check-m0-templates.sh`: **9 Go-template oracle cases**, passed with Go 1.27.1. Standard library only, `GOPROXY=off`, `GOSUMDB=off`, local toolchain; separate from application build/CI requirements.
- Linux x86_64, Rust 1.97.0, GPUI Kit 0.7.1, installed Git 2.56.0. No dependency/toolkit upgrade used to bypass input behavior.
- Git operations used disposable temporary repositories, linked worktrees, a local submodule and a local bare repository only. Fixtures clear inherited environment and isolate HOME/XDG/global/system Git config, hooks, signing and credential helpers. No user repository or account was used.
- No native GUI launch, desktop automation, credential/keyring/agent access, network repository access or shared LazyGit configuration writes.

## Exit-gate evidence

| M0 gate | Production prototype and automated evidence | Result |
| --- | --- | --- |
| Keyboard/control | `views/commit_controls.rs`; `views/tests/commit_controls.rs`: real Kit Input/Textarea in Root, subject Enter, body newline, Tab, Ctrl+S/Ctrl+Enter, editing arrows, Ctrl+O menu, menu arrows, Escape restoration/cancel, text/menu global suppression, marked composition and text insertion | Headless feasible; native acceptance separate |
| Git/process | `connectors/git/`; `tests/git.rs`, `tests/process.rs`: repository identity, linked `.git` file/common-dir, bare/submodule/unborn/nonrepo, NUL status including rename and byte paths, injected executor, bounded output, owned process-group cancellation/direct-child reap | Feasible for owned non-daemonizing processes |
| Window shutdown | `views/process_probe.rs`, `views/tests/process_probe.rs`: closing a real headless Root releases the injected non-rendering operation entity and reaps its active disposable child | Safety demonstrated; production asynchronous shutdown wiring is M1 |
| Patches | `diff/`, `working_tree/`; `diff/tests/patch.rs`, `working_tree/tests/{barrier,stale}.rs`: selected replacements/deletions/additions, reverse partial unstage, preserved worktree bytes/other staged changes, whole-file add/delete/rename operations, stale-target rejection, two queued actions resolved after refresh | Canonical text approach feasible; fallback boundaries explicit |
| Config/templates | `storage/lazygit.rs`, `storage/tests/{lazygit,trust,diagnostics}.rs`, `input/tests/resolution.rs`; full captured defaults parse, supplied-field merge, newer commands first, scalar/list/disabled/legacy bindings, precedence, transactional reload, provenance/diagnostics; Go corpus below | Narrow implementation route selected; no full config/template parity claim |
| Prompt bridge | `connectors/prompts/`, `tests/{bridge,lifetime}.rs`: fake askpass/editor/sequence-editor, abort, forbidden file, forged token, replay, message bound, timeout/cancellation, operation-close waiter settlement, permissions/cleanup | Private operation-scoped transport feasible; not a live Git helper integration |
| Approved policies | `input/` diagnostics/action contracts, config trust fingerprints, `repository/Navigation` and its owner-local test | Applied at prototype/contract level; future product controls must use these contracts |

## Closed implementation decisions

### Input and native controls

Keep Kit 0.7.1. Use a normalized key representation and context resolver rather than translating LazyGit strings directly to widget bindings. The captured Linux defaults exercise both `ctrl+key` and `alt-key` grammar, Backtab, disabled/scalar/list bindings and legacy `-alt` names.

**Bound actions and raw keys are different paths.** The initial control journey missed Ctrl+Enter when it listened only for raw key events. Capturing Kit's `Enter` action and `IndentInline` action, then routing through the same resolver, fixes the regression. Subject/body/menu contexts are exclusive; a global custom binding cannot overtake a context-specific built-in. A same-context custom binding can. Body ordinary Enter stays with Textarea; Ctrl+Enter and Ctrl+S emit the same submit intention. Marked input suppresses submission. The view emits intentions only: commit workflow/draft retention and double-submit guards remain with the future commit owner.

The menu is a local feasibility surface, not a global dialog registry or completed upstream menu. Arrow-history recall, full focus/context stack, real clipboard/paste events, accessibility, IME candidate handling and layouts still need the M1/native journeys. Synthetic marked-text tests do not prove native IME acceptance.

### Git, process ownership and shutdown

Keep the installed Git CLI authoritative. Repository identity consists of optional worktree, resolved Git directory and common Git directory; path/status identities preserve Linux bytes. Built-ins use argv and `--`, not shell interpolation. Canonical diffs disable external diff/textconv/color. `--no-index` accepts exit 1 as differences; other commands do not inherit that exception. Optional index locks are disabled only for background reads.

`Executor` is a Git/process mechanics substitution seam, not a universal connector. `Native` starts a private process group and drains bounded stdout/stderr (1 MiB each, truncation causes Git result rejection). Payloads/output do not implement Debug or enter command logs. Cancellation kills only the owned group and reaps the direct child. Linux `waitid(WNOWAIT)` retains the leader's PID until the group is settled, avoiding a kill-after-reap/PID-reuse race. Child status is not proof that writes did not occur; callers reconcile, never automatically retry a mutation.

`Operation` retains its worker; a view replacement must not release the operation's feature owner. Its cancellation token also terminates operation-scoped prompt waits. **The prototype's Drop joins synchronously. Do not ship that Drop path on the GPUI thread.** M1 must retain a narrowly scoped operation/shutdown holder: window-close requests cancellation or command-specific completion, the worker settles helpers/children, and startup keeps the holder until shutdown is acknowledged. The last-window exit must wait for that acknowledgment. No sample six-second deadline is inherited for writes/prompts.

The tested contract covers children/helpers that remain in the owned process group. Daemonizing/escaping helpers are not established safe: process groups are not a sandbox, and arbitrary unknown command strings cannot prove noninteraction or containment. M1/M5 tool admission must require explicit nonterminal/non-daemonizing capabilities, closed stdin except documented file/patch payloads, and diagnostics for unknown/terminal tools. No PTY/terminal fallback. Descendants killed with their group can briefly remain zombies awaiting the OS reaper; the direct child is explicitly reaped.

**Git support floor:** only 2.56.0 is validated here. Start M1 with that conservative floor and a visible version/readiness check; do not infer support for older Git merely from individual flag introduction dates. Lowering the floor needs the same fixture matrix on that version.

### Canonical patch targeting and refresh barrier

Keep canonical bytes separate from presentation. A `Patch` scopes change ordinals to the exact raw snapshot. `working_tree::apply_selection` re-reads and rejects a stale snapshot before dispatch. Display offsets/wrapping/highlighting never become operation targets. A reverse operation first transforms the source/destination interpretation; reversing a forward-selected subset is not equivalent.

Complete added/deleted-file selections fall back to file operations, preserving absent/untracked semantics rather than creating empty tracked files. Whole-file rename operations use both affected path identities. The single-file text prototype deliberately rejects binary, symlink/gitlink/mode-only, rename-metadata and missing-final-newline patches for partial transformation; report a whole-file fallback, never apply lossy display text. Partial new/deleted files, richer patch headers, zero-context selection policy and exhaustive boundary/property cases must be added before the M1 partial-operation matrix is claimed complete.

`Barrier` stores queued **intentions**, not cached row/patch targets. Completion releases the next intention only after a fresh authoritative read and target/focus reconciliation. Two rapid actions survive and select the next remaining change. Pane rendering/focus after the last change is M1 work.

M1 must add shared resource gates before shipping writes: index/worktree gate keyed by canonical worktree identity; refs/history-operation gate keyed by common Git directory. Hold the relevant gate across target verification, dispatch and reconciliation. All windows obtain the same resource handles from a narrow retained holder. External Git remains authoritative and can still race; do not promise a universal transactional snapshot or retry on uncertainty.

### Read-only config, precedence and trust

Keep shared LazyGit files read-only. Merge only supplied fields, replace ordinary lists, prepend newer custom commands and branch color patterns, normalize scalar/list bindings, drop empty/disabled entries and union legacy alternates. Config reload builds/validates a complete candidate before replacement. Every supplied setting has file/path-aware feedback; settings are not silently applied to the catalog. Syntax errors retain parser line/column; per-field YAML line spans are not implemented.

The prototype receives ordered `Source` values; it does **not** yet implement CONFIG_DIR/XDG/LG_CONFIG_FILE/CLI source discovery, ancestor ordering, all in-memory migrations, all 362 fields or full Go YAML equivalence. Unsupported/unknown settings remain diagnosable. Do not describe generic merge tests as complete upstream configuration parity.

`Trust` remembers approval by source and SHA-256 of the executable configuration projection, stored atomically in a **separate GUI-only file**. It stores byte paths/fingerprints, not command text. Restarted approvals work, executable changes renew approval, ordinary GUI-only changes do not. Explicitly selected global sources are trusted. Config discovery must supply canonical source identities. The current projection covers custom command definitions (including prompts/suggestions/templates) and OS tools; classify any additional executable setting before introducing its execution entry point. Trust is checked before template/suggestion/menu resolution, not just before a final command. M1 integrates these file mechanics into the one ordered GUI storage worker; this prototype is not multi-process storage coordination.

### Go templates: narrowest viable route

The corpus in `spikes/templates/tests/cases.json` validates selected objects, `.Form`, shell quoting/pipelines, variables/conditionals/built-in comparison/formatting, ranges and nested menu data, fixture-only `runCommand`, missing-field semantics, parse/function errors and pre-evaluation trust rejection. All nine pass using standard Go `text/template`.

**Recommendation:** use a narrow Go standard-library evaluator/helper for full compatibility rather than inventing interpolation or silently translating languages. Rust remains the owner of context, trust, prompts and operation execution; a future helper must request `runCommand` through a guarded operation protocol, not launch arbitrary commands itself. Do not select an unvalidated Rust evaluator as if it were equivalent.

This oracle is **not** a production dependency/helper, does not import/build LazyGit, and never executes a shell. Production Go build/distribution remains subject to the plan's separate approval requirement. Until approved and integrated (M5), custom commands/templates must be explicitly unavailable. The corpus proves a route, not complete object/function/menu compatibility. No license grant or copied upstream production implementation was introduced.

### Prompt protocol and approved policies

Use a per-operation private directory (0700), socket (0600), random 256-bit token, monotonically validated request IDs, bounded frames and cancellation/deadline checks. Bind editor/sequence-editor requests to the explicitly allowed operation file. Invalid authentication/replay/file/size never dispatches to a UI callback. Abort is explicit and does not modify the file. Operation close ends idle waiters and removes the socket. The prototype client uses a short test-oriented response timeout; real native prompts need operation-specific wait/cancel semantics.

This is fake IPC feasibility, not installed GIT_EDITOR/GIT_ASKPASS executables. The M1/M4 adapters must authenticate their helper handoff, route requests to the owning feature, preserve file-return/exit-code semantics, settle callbacks on cancel, and keep secrets out of diagnostics. Same-UID processes that can inspect memory/private environment are outside this IPC authentication boundary. It is not a credential vault or OS-user sandbox.

HTTPS Git askpass, OpenSSH askpass/host-key policy, and signing/pinentry are separate. Existing graphical agents/pinentry remain authoritative; no insecure host-key override and no promise that HTTPS askpass solves SSH/signing. Live provider tests require separate explicit consent.

Policy contracts recognize unsupported shared-config editing, suspend, custom renderers, terminal/PTY output, updates and decorative actions; diagnose non-English configuration rather than choosing an unnoticed fallback. Quit resolves to a **window-local** close intention, never shell cwd changes or suspension. The shell/owner must still honor configured quit confirmation and retained shutdown. `repository::Navigation` switches in place with a submodule parent-return stack and generation rejection; it does not add tabs or another product-wide state bag.

## M1 handoff

M0's six spikes and decision log are complete. The above limitations are explicit implementation/acceptance work, not newly approved product exceptions. Before a usable stage/commit loop ships: replace the catalog as a connected slice; wire repository readiness/version checks, shared mutation gates, retained asynchronous shutdown, ordered config/trust storage, authoritative working-tree/commit owners and contextual controls; extend conservative partial-patch fallbacks; and run the complete M1 matrix. Keep unavailable workflows visibly unavailable. Native/user acceptance remains separate.
