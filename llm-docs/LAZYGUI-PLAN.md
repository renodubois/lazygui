# LazyGUI implementation proposal

Status: **M0 complete; M1–M5 proposed**. Product decisions/evidence: [LAZYGIT-INVESTIGATION.md](LAZYGIT-INVESTIGATION.md). [M0-RESULTS.md](M0-RESULTS.md) records the headless prototypes, decisions, passing checks and explicit limitations. The executable remains the catalog starter; no usable Git client or native parity is claimed. No duration estimates are promised.

## Design constraints

- Use the installed Git CLI; preserve repository config/hooks/filters/signing behavior except explicit terminal restrictions. Avoid libgit2 as the authoritative operation engine for this compatibility target.
- Render with GPUI Kit 0.7.1. Keep current dependency versions initially; do not upgrade the toolkit to evade an untested keyboard issue.
- Native unified diff, distinct unstaged/index panes; not terminal emulation and not old/new side-by-side. Custom renderers/cycling and PTY output are unsupported.
- Preserve actual contextual command intentions, predicates, selection/focus consequences, dialogs and errors. A matching key label alone is insufficient.
- Do not import LazyGit's God GUI/common dependency bags, introduce an event bus, universal connector, all-purpose state struct or global dialog registry.
- All tests reside under owning modules' `tests/`; cross-feature helpers in test-only `src/test_support/`. Never widen production visibility just to relocate suites.

## Proposed ownership map

Paths below describe the product ownership target. Some now contain M0 capability prototypes compiled through `src/lib.rs`, not startup integrations; see [M0 results](M0-RESULTS.md). Introduce later feature modules only when their workflows are implemented, not an empty full-product scaffold in M1.

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
| `views/working_tree/`, `views/commit/`, later owner views | Native panels/dialogs and editable control state connected to feature intentions | Direct Git commands, duplicate authoritative drafts/results |
| `views/app_shell.rs` | Retains window feature owners and views; delivers each owner's opaque outcomes once | Interpreting all feature results or centralized product-dialog policy |
| `runtime.rs` | Execution/time substitution | Global business jobs/settings |

A shared process-lifetime holder may be justified for multiwindow opening, a single GUI preference writer and retained operation shutdown. If introduced, give it only those explicit responsibilities; do not call it `AppState` and move all state into it. Shared preferences must not acquire one unsynchronized writer per window.

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
- Determine a tested minimum Git version from used commands/flags; baseline Git 2.56.0 is observed, not an automatically justified universal minimum.
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

### M1 — first usable inspect/stage/commit loop

- CLI open by cwd/`--path`, one active repo/window; safe nonrepo/error state. Repository switch actions replace the current repository in place; later submodule Enter/Esc retains its parent-return stack. Ensure stale outcomes from a previous identity cannot populate the new session.
- Recognizable five-panel skeleton and tab labels; unavailable sections/actions explicitly labeled. Files and diffs functional; status/current branch and a minimal read-only commit list for feedback. No fake Git example rows.
- Files tree/flat mode, basic text/status filtering, selection/range navigation, global panel jumps/help/refresh/clipboard/screen layout as needed for the loop.
- Unstaged/staged unified panes with correct focus/empty state; hunk/line/range mode, partial and whole-file stage/unstage, context/whitespace controls.
- Native commit subject/body, cancel/retry draft retention, ordinary hooks and commit results, configured warnings and message rules. Advanced amend/fixup/editor variants can be marked unavailable until their stage.
- Refresh on actions/external modifications, with stale-result protection and rapid-key reconciliation. Default auto-refresh/external detection policy represented; network auto-fetch not silently launched before M3. Report this temporary gap.
- Configuration subset needed for this milestone honored; the rest diagnosed. Full `customCommands` compatibility is **not** claimed here.

#### M1 automated acceptance matrix

| Journey | Required assertions |
| --- | --- |
| Open clean/unborn/detached/bare/worktree repository | Correct identity/readiness; graceful empty states; no destructive command in unsupported contexts |
| Files `2` → arrows/jk → Enter/`0` → Esc | Correct file target, directory expansion versus diff focus, return target and contextual help |
| Mixed staged/unstaged file → Tab → Space | Correct index side changes; worktree bytes unchanged by stage/unstage; pane identity stable |
| Hunk mode → `a` line mode → `v`/Shift+arrows → Space | Only selected changes reach/leave index; anchors and mode semantics match |
| Rapid Space Space | Both intents processed against refreshed targets; next surviving hunk selected, not stale rows |
| Stage last change / unstage last staged change | Empty pane hidden, focus follows remaining pane; all-change delete/add becomes deleted/untracked correctly |
| Rename/new/delete/binary/symlink/mode-only | Whole-file operations correct; unsupported partial operations clearly disabled, never lossy text patch |
| Paths with spaces/tab/newline/leading dash/non-UTF8 | Correct decoding/argv/path identity; display encoding never changes operation target |
| External file/index change during selected action | Reconcile or reject stale target; don't apply a patch to unrelated replacement contents |
| Commit subject Enter / Tab body / Ctrl+S or Ctrl+Enter | Correct submission/newline/focus behavior; no double submit |
| Cancel or failed fake hook | Draft preserved, accurate error; no implied success or automatic retry; reopen and correct retry works |
| No staged files | Upstream warning/stage-choice sequence honored, including configured warning skip behavior |
| IME/paste/text editing | Composition/text/paste never fires global repository actions; include fake headless input and manual native verification |
| Popup/search/copy | Context-specific precedence; Ctrl+O commit menu vs copy elsewhere; no underlying command leaks |
| Refresh/out-of-order/drop/recreate | Only current generation applies; one result consumer; no duplicate initial work |
| Child error/cancel/close | Owned subprocesses settled/reaped under established policy; uncertain writes reported accurately |

Partial discard/reset commands can be a follow-on rather than a first-release dependency. When added, staged `d` is unstage, unstaged `d` is configured confirmed discard. Do not substitute generic GUI Delete behavior.

#### M1 user acceptance

Human uses disposable repository snapshots and a printed key journey to compare stock v0.66.0 and LazyGUI. Record focus transitions, selection after each operation, repeated-key behavior, dialogs, errors, result index/tree and commit message—not just screenshot resemblance. No benchmark claims until measured; set performance budgets from representative repository fixtures. X11/Wayland, IME/layouts, clipboard, keyboard-only operation and accessibility are separate checklist entries.

The user drives native acceptance. Agent GUI launch/automation/real keyring access remains prohibited without separate explicit consent and an unlocked isolated test session.

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

## Template replacement

Follow [CUSTOMIZE.md](CUSTOMIZE.md) when implementation starts: replace the catalog owner/connector/views as a connected slice, not by simply renaming catalog records to files. Remove demo HTTP environment selection and reqwest only once references disappear; don't inherit sample fixed deadlines or serialized query persistence as Git operation policy. Update OVERVIEW/ARCHITECTURE/EXTENDING/VERIFY to actual implemented behavior as slices land.
