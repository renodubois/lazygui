# LazyGUI implementation proposal

Status: **M0 complete; M1 automated acceptance passed (188 tests), native core loop user-confirmed, extended checklist pending; M2–M5 proposed**. Product decisions/evidence: [LAZYGIT-INVESTIGATION.md](LAZYGIT-INVESTIGATION.md). [M0-RESULTS.md](M0-RESULTS.md) preserves historical prototypes/baseline; [M1-RESULTS.md](M1-RESULTS.md) records the connected Git implementation, exact acceptance suites/check output and remaining gaps. Startup no longer runs the catalog. The user confirmed the native core loop on 2026-10-09; extended native acceptance remains pending. No full LazyGit/configuration parity or duration estimates are promised.

## Design constraints

- Use the installed Git CLI; preserve repository config/hooks/filters/signing behavior except explicit terminal restrictions. Avoid libgit2 as the authoritative operation engine for this compatibility target.
- Render with GPUI Kit 0.7.1. Keep current dependency versions initially; do not upgrade the toolkit to evade an untested keyboard issue.
- Native unified diff, distinct unstaged/index panes; not terminal emulation and not old/new side-by-side. Custom renderers/cycling and PTY output are unsupported.
- Preserve actual contextual command intentions, predicates, selection/focus consequences, dialogs and errors. A matching key label alone is insufficient.
- Do not import LazyGit's God GUI/common dependency bags, introduce an event bus, universal connector, all-purpose state struct or global dialog registry.
- All tests reside under owning modules' `tests/`; cross-feature helpers in test-only `src/test_support/`. Never widen production visibility just to relocate suites.

## Ownership map: installed M1 and future scope

[ARCHITECTURE.md](ARCHITECTURE.md) maps the actual installed tree. Repository, working-tree, commit and history owners now connect through production startup/shell/views and `src/lib.rs`; future feature paths below remain targets, not implemented scaffolds. M0 prompt/submodule-navigation/template probes remain feasibility evidence, not installed integrations.

| Module | Owns | Must not own |
| --- | --- | --- |
| `main.rs` | CLI parsing/startup dependencies, toolkit setup, opening repository windows | Git workflow policy, panel rendering |
| `repository/` | Immutable discovered repository/worktree identity, capability/readiness and refresh invalidation coordination | Every file/branch/commit/draft in one aggregate state |
| `working_tree/` | Files/index state, snapshots, staging/discard intentions, selection-target resolution and post-write refresh barriers | Controls, process spawning/decoding |
| `commit/` | Commit draft lifetime, submit/amend policy, hook/signing results and draft retention | Text-widget caret/IME state |
| `history/` | History/ref reads and longer-lived filter/workflow data | View-local scroll/focus |
| `refs/`, `stash/`, `sync/` | Respective authoritative data and operations as introduced | Catch-all app state |
| `rewrite/`, `conflicts/` | Rebase/custom-patch/cherry-pick/undo and conflict workflow coordination | Global dialog infrastructure |
| `custom_commands/` | Command definitions/context snapshots, prompt sequence, template evaluation policy and execution outcomes | Generic app-wide scripting/event bus |
| `input/` | LazyGit key grammar, normalization, context/action resolution and help metadata (pure where possible) | Git I/O or long-lived business state |
| `diff/` | Raw patch parsing, stable change identities, selected-patch transformation (pure) | Screen row selection, commands that mutate repositories |
| `connectors/git/` | Capability-specific typed client operations, Git argv/output codecs, subprocess mechanics and per-repository mutation gate | Rendered feedback, confirmation decisions |
| `connectors/tools/` | Nonterminal editor/browser/shell execution; explicit tool capabilities | Arbitrary guesses about whether a command will interact |
| `connectors/prompts/` | Private editor/sequence-editor/askpass request transport, cancellation and redaction | Credential storage, universal prompt abstraction |
| `storage/` | Read-only LazyGit config discovery/parsing; separate ordered LazyGUI preferences and approved source-specific trust records | Git repository data cache or workflow decisions |
| `views/repository/` | Layout/context stack, focus, local selection/scroll, contextual action dispatch and private child views | Transport access, authoritative workflow decisions |
| `views/commit_controls.rs`, repository-private panels and later owner views | Native controls/dialogs and editable control state connected to feature intentions | Direct Git commands, duplicate authoritative drafts/results |
| `views/app_shell/mod.rs` | Retains window feature owners and views; delivers each owner's opaque outcomes once | Interpreting all feature results or centralized product-dialog policy |
| `connectors/git/process.rs` | Installed worker/process execution substitution and retained workflow shutdown; GPUI shell owns its timer delivery | Global business jobs/settings |

M1 startup constructs one shared `ProcessHost`, `MutationGates` and `OrderedStorage`, not an `AppState`. Owners admit retained workflow scopes before spawning; close asynchronously awaits settlement through cleanup/delivery and rechecks last-window removal. Preferences/trust writes are storage capabilities, but production currently only reads window size/trust and flushes: save/approve/revoke UI wiring remains open. Never create unsynchronized per-window writers/gates.

### Lifetime and consistency

- Bind clients to discovered worktree and common Git-dir identity; recognize a `.git` file/linked worktree, bare repo, explicit worktree/git-dir and submodule context. Don't assume `<path>/.git` is a directory.
- Window owners retain business state while views are recreated. Selected file identity and patch targets must not be serialized as screen offsets.
- Read generations reject stale outcomes even when cancellation fails. Coalesce automatic refreshes; allow explicit refresh; preserve prior usable data on failure.
- Mutation gate is scoped to actual repository resources: index/worktree per worktree; shared refs/rebase-relevant coordination for a common Git directory. Multiple windows must not each assume sole write ownership. Git locking remains authoritative; external tools aren't controlled by the GUI gate.
- A write command uses a confirmed target snapshot. If it becomes stale, refresh/reconcile before executing; never stage a newly occupying display row.
- After staging, reconcile raw diff, target selection and pane focus before dispatching queued target-dependent keys. This must preserve rapid repeated actions without blocking the entire GPUI event loop.
- No automatic write retry on timeout/cancel/uncertain outcome. Check authoritative repository state and report what is known. A process error is not proof no writes occurred (hooks/filters/multi-file actions may have side effects).
- Closing/dropping a view is not canceling a business operation. Dropping a Tokio task is not a complete child-process kill/reap protocol. M0 must establish ownership through window-close/shutdown and helper-request lifetimes before writes ship.
- Mutations should have command-specific cancellation semantics, not inherit the sample six-second read deadline. Native editor/credential prompts can legitimately take longer. Cancellation must settle helpers and reap owned children/process groups without touching unrelated processes.

### Git boundary

- Built-in operations construct argv directly with explicit cwd/repository binding and `--` path separation; shell strings are reserved for configured custom/tool commands whose contract requires shell evaluation.
- Preserve Linux path bytes where possible; NUL-separated output parsing must handle spaces, tabs, newlines, leading dash names and rename old/new paths. Human-oriented/ANSI output is not a status protocol.
- Distinguish binary diffs, modes/symlinks/gitlinks, textconv/external diff display, and canonical applicable patches. Native syntax highlighting is presentation only.
- Use typed outputs and typed exit/error states. Diff `--no-index` has a meaningful nonzero differences result; do not classify all nonzero exits identically.
- M1 enforces Git 2.56.0 as a conservative tested support floor before installing owners. This does not claim every flag intrinsically needs that version or older Git has been validated.
- Background reads should avoid optional index-lock contention as upstream does. Do not set blanket environment overrides that unintentionally alter foreground operation behavior.
- Command logs need bounded buffers and redaction. Never log secrets, credentials in URLs, editor/askpass IPC payloads, or unbounded hook output. A configured shell command is not sandboxed.

### Configuration and actions

- Keep default snapshot, shared config sources and GUI preferences distinct. No shared-file creation/migration/writes.
- Preserve supplied-field merge semantics, key aliases/legacy alternates, array versus scalar bindings, custom-command accumulation and source locations for diagnostics.
- Transactionally validate/reload settings/keymap; retain the previous valid state on reload failure and report it visibly.
- Use one action-definition/resolution source for keyboard handlers, contextual help and clickable controls; context-specific guard and custom-command precedence are explicit.
- Map input/IME/paste separately from action keystrokes. Typed `q`, `c` or `Space` in a commit message must not invoke repository commands. Composition Enter must not commit. A popup/search prompt must suppress underlying commands.
- Full Go-template language/object/function compatibility needs a spike. A Rust evaluator must be tested for Go semantics; a dedicated Go helper would add a build/distribution boundary and must be approved before becoming a dependency. Do not claim an interpolation-only parser is equivalent.
- Template `runCommand`, command menus/suggestions and ancestor repo config create execution/trust risks. Newly encountered executable repo/ancestor configuration requires remembered source-specific approval, renewed on executable configuration changes; ordinary settings may load and explicitly chosen global config is trusted. Guard all execution entry points, not just final commands. Shared-config edit shortcuts report unsupported.

### Native tool/prompt strategy

- Native commit body, reword and todo/patch editor workflows are required equivalents. Use a private authenticated helper bridge for Git's editor/sequence-editor requests where a file-return protocol is required; include wait/abort behavior and temporary-file cleanup.
- Configured graphical editors/tools remain possible; commands known to request terminal output or configured as terminal editors are rejected. Unknown command strings cannot be proven noninteractive: provide explicit capability metadata/failure diagnostics, closed stdin/non-PTY behavior and a tested shutdown path, never an undocumented terminal fallback.
- Noninteractive hooks still execute through Git. Do not secretly add `--no-verify` to make them pass; hook skip is only the upstream command/config choice.
- Git askpass, SSH askpass and signing/pinentry are separate integrations. Reuse graphical pinentry/agents; preserve security (SSH host-key verification included). Fake prompts in automation; live keyring/agent consent separate.
- Proposed prompt IPC should be private per operation with strong request identity, narrow permissions, bounded messages and explicit timeout/cancel. Another process must not be able to steal a passphrase request or supply a forged editor response.

## Milestones and exit gates

### M0 — feasibility and contract closure (not a usable Git client)

**Completed at the feasibility/contract level.** All six spikes have evidence and decisions in [M0-RESULTS.md](M0-RESULTS.md); 45 Rust tests and 9 separate Go-template oracle cases passed. Native acceptance and the M1 product integrations remain unclaimed.

Resolve evidence-backed risks before building every panel:

1. **Keyboard/control spike:** real Kit controls in headless Root; subject/body, Tab, Ctrl+S/Ctrl+Enter, arrows, menus, Escape stack and global suppression. User later verifies composition/layout-dependent behavior manually.
2. **Git/process spike:** discovery, typed status/diff, controlled executor/process seams, spawning/kill/reap and window close while a disposable child is active. Ensure no orphan helper waits.
3. **Patch spike:** canonical patch model; stage/unstage additions, deletions, replacements and renamed/new files in temporary repos; complete-file selection fallback; stale target and rapid-action barrier.
4. **Config/template spike:** reproduce merge/binding precedence and validate realistic Go templates. Recommend the narrowest viable implementation; record limitations instead of silently losing compatibility.
5. **Prompt bridge design/prototype:** fake askpass/editor requests only; establish why SSH/signing are not assumed solved by HTTPS askpass.
6. **Apply the approved policies** from the investigation: unsupported config editing/suspend/renderers/PTY/updates/decorative extras, in-place repository transitions, source-specific executable-config trust, window-local quit and English-only UI. Include clear action/setting diagnostics and no silent fallback.

Exit: decisions logged, headless feasibility evidence, module interfaces agreed, remaining incompatibilities explicit. No native-parity claims based on compilation alone.

### M1 — connected inspect/stage/commit loop

**Implemented; M1 automated matrix and final regressions passed (188 tests); native core loop user-confirmed, extended checklist pending.** Six final full checks passed after resolving fixture shutdown waits; see M1 results for coverage and bounded limitations. The executable now opens real repositories; removed demo/catalog/HTTP suites are not included in the current count.

- cwd/`--path`, byte-preserving `--use-config-file` and config discovery; safe nonrepo/unsupported-Git state, in-place switching and stale identity isolation.
- Five native panels with functional files/diffs, current branch/status and minimal raw-message read-only history. Unavailable tabs/actions are labeled; no fake Git rows.
- Tree/flat/text/status filters, visible ranges, contextual panel/help/copy/layout controls. Key/help/click precedence is scoped to the owning view/control, not a universal executor.
- Canonical unstaged/index unified panes; hunk/line/sticky/Shift ranges, partial/whole stage/unstage and rapid-key reconciliation. Safe partial new/delete regular text; explicit no-newline/binary/symlink/gitlink/rename/mode/zero-context/whitespace fallback.
- Retained native commit draft, subject/body/IME/popup suppression, cancel/reopen/failure/deliberate retry, hooks, stdin message, signoff/wrapping/full recall/no-stage warning and configured branch/hook-skip prefixes.
- Coalesced polling refresh behind slow reads/queued mutations, explicit refresh, authoritative reconciliation and no uncertain-write replay.
- Shared canonical index/worktree/common-directory gates and retained pre-spawn workflow admission through cleanup/delivery; nonblocking owner/operation Drop and acknowledged last-window shutdown.
- Read-only shared config with transactional reload/source diagnostics. Supported consumer subset distinguished from parsed unavailable settings; **not exact full config parity**. Prefix regex accepts a conservative Go-compatible subset; invalid/unsupported constructs transactionally fail, never silently reinterpret.
- Separate ordered GUI preferences/trust storage capability. Startup reads window size/trust and close flushes, but automatic preference saves/recent paths and trust approval UI are not connected.
- Network auto-fetch deferred/diagnosed until M3; `customCommands`/templates unavailable. A production Go/`templatesGo` helper remains unapproved.

#### M1 automated acceptance matrix

The complete journey-by-journey matrix, exact owning suites/test names, assertions and check transcript are maintained in [M1-RESULTS.md](M1-RESULTS.md#automated-acceptance-matrix). It covers opening/readiness, files/directory navigation, mixed panes, partial/range/rapid keys, last-change focus, metadata/byte paths/staleness, real commit controls/hooks/retries/no-stage/message policy, synthetic IME/popup precedence, refresh/reload/out-of-order/drop, gates/close and read-only config/storage.

Installed-Git journeys use disposable isolated repositories through the production readiness/shell/owner constructors and real headless Kit controls. Lower-level controlled suites verify the same owner coordination under held errors/cancel/panic/time, not test-only duplicate workflows. Physical input, native layout/accessibility/provider behavior cannot be inferred from these tests.

Partial discard/reset is follow-on, not shipped M1 behavior; staged `d`/unstaged confirmed discard semantics must be implemented explicitly when added.

#### M1 human acceptance

On 2026-10-09 the user confirmed the native inspect/stage/unstage/commit core loop; environment and detailed journeys were not supplied. This does not establish full-checklist acceptance. The concise disposable human checklist is in [VERIFY.md](VERIFY.md#disposable-human-acceptance-checklist--pending). Compare stock v0.66.0 focus/selection/index/worktree/commit outcomes, not screenshot resemblance. X11/Wayland, real IME/layouts, clipboard, keyboard-only operation, accessibility and responsive close remain pending. No measured performance/packaging claims.

Native launch/automation/keyrings require separate explicit consent and an unlocked isolated test session; the agent performed no native launch; the user-reported core-loop test is recorded separately.

### M2 — everyday local repository workflows

Branch/history/reflog exploration, checkout/create/rename/delete, ordinary amend, ignores/excludes/discards/resets, stash variations, richer graph/filter/search and recent repos. Implement confirmation/error/source-target semantics with each action, not later.

### M3 — synchronization and repository topology

Fetch/pull/push/upstream/force/tag/remote/browser workflows; no network testing against real accounts. Add worktrees/submodules and in-place switching and submodule parent-return behavior, auto-fetch policy and stacked-branch update/push discovery. Shared Git-dir coordination validated with multiple windows and external Git writes.

### M4 — advanced history, patches and conflicts

Interactive rebase/todo/message editing, squash/fixup/reword/drop/move/cherry-pick/revert, amend-to-past-commit and root/merge cases; custom patches, bisect, conflict editing and resolution undo, global reflog undo/redo, full stacked-branch behavior and Git-flow integration where applicable.

### M5 — compatibility closure

Complete supported config/Go-template/custom-command/output compatibility; graphical editor/tool variants, remaining menus/input bindings/CLI flags, and explicit exception diagnostics. English only: translations, custom renderers/cycling, `terminal`/`logWithPty`, shared-config editing, suspend/cwd, updates and decorative extras are excluded. Audit all 301 documented rows plus source-derived missing bindings, guards and menus. This stage is a maintained checklist, not a vague polish bucket that can erase missing functionality.

## Parity accounting

For each action build a reviewed record containing:

- Pinned controller/context and config key; semantic action ID, aliases, precedence, conditions/disabled reasons.
- Target prerequisites, command/argv/env, warnings/dialog/prompt/menu sequence and cancellation behavior.
- Raw Git state/result expectations, refresh scope, selection/focus after success/failure.
- Headless test IDs and manual journey result; exception with explicit user approval when applicable.

Statuses: discovered → semantically reviewed → implemented → automated verified → native accepted, or explicit approved exception. The extracted research indexes deliberately start pending; they are not executable product manifests. Do not use one universal action executor to route every feature operation.

A release names its supported workflows and gaps. Final compatibility must account for config-defined/dynamic actions and unknown settings as well as default cheatsheet rows. No claim of original unqualified full parity is possible with the approved exceptions. State compatibility against v0.66.0 plus the explicit policy-resolution table, not an unrestricted entire-feature-set promise.

## Safe automated verification

- Run `./scripts/check.sh` from the repository after changes; retain locked dependencies/owner-local suites.
- Owner tests use fake requests/process outcomes/time. Connector codec/operation tests may use the installed Git binary **only in disposable temporary repositories**, exercising production connectors rather than recreating product workflows in tests.
- Isolate `HOME`, XDG paths, Git global/system configuration, hooks/signing/credential programs, inherited repository/env overrides and remotes. Install only fixture-owned hooks/agents/helpers where a test requires them; never inherit a real user's provider or account.
- Remote tests use disposable local bare repositories or fake/owned loopback listeners only. No public Git hosting, real signing keys or desktop/keyring access.
- Cross-feature test fixtures belong in test-only `src/test_support/`; single-suite/owner-shared helpers stay local.

## Completed starter replacement / current customization

M1 replaced the catalog/HTTP example as a connected Git slice; demo environment selection, query persistence and sample runtime are no longer startup behavior. [CUSTOMIZE.md](CUSTOMIZE.md) describes current naming/ownership changes. Historical M0/template evidence remains preserved, but old generation smoke tests do not validate the renamed product/library/title. Do not inherit fixed sample deadlines or query persistence as Git policy.
