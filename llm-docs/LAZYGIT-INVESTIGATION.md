# LazyGit → LazyGUI compatibility investigation

Research date: 2026-10-08. Status: source-grounded planning, **not implementation or a parity claim**.

## Baseline and evidence

- Installed LazyGit: v0.66.0, Linux amd64, binaryRelease; installed Git: 2.56.0.
- Upstream annotated tag v0.66.0 resolves to commit `c5f7158154602d23b0750d4304c73e2aead8df5b`. Research used a shallow detached checkout outside this repository; no upstream build or tests were run.
- The user's standard LazyGit config is empty; the user confirmed no alternate/repository-specific configuration in their current usage.
- Linux defaults were captured with `lazygit --config`. This exports defaults, not the user's effective merged configuration.
- Application remains the Rust/GPUI catalog starter. `./scripts/check.sh` passed: format, strict clippy, **20 tests**, build. This establishes starter health only.
- No native application launches, desktop automation, real credential-provider access, or user-repository mutations were performed.

### Durable evidence index

See [research inventory](research/lazygit-v0.66.0/OVERVIEW.md):

| Artifact | What it establishes | Count |
| --- | --- | ---: |
| `bindings.csv` | Every action row in the pinned generated English cheatsheet, retaining context, key label, information and source line | 301 rows / 22 sections |
| `config-fields.csv` | Leaf/recursive paths extracted from the pinned JSON schema, including OS/custom-command settings absent from the default export | 362 rows |
| `workflow-functions.csv` | Production function signatures in controllers, custom commands, Git operations and patch code; source navigation aid | 1,975 |
| `upstream-test-files.csv` | Integration Go file paths, including helpers and demos; a scenario-discovery seed | 677 |
| `defaults-linux.yml` | Complete Linux default export | 379 lines |
| `manifest.json` | Commit, extraction counts, limitations and source hashes | — |

**These counts are not feature counts or coverage percentages.** The cheatsheet omits some navigation/editor bindings; menu actions, guards, context attachments and dynamic custom commands require source inspection. Function/test-file indexes have not all been semantically reviewed. Every extracted binding starts `not_implemented`, semantic review/acceptance pending. No tests were ported.

### Pinned primary sources

All links below use the exact commit, not moving master:

- [K: generated bindings](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/keybindings/Keybindings_en.md)
- [D: default values and types](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/config/user_config.go), [schema](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/schema/config.json)
- [C: config loading/migration](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/config/app_config.go), [repository config discovery](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/gui.go#L484)
- [B: runtime binding dispatch](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/keybindings.go), [controller/context attachment](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/controllers.go)
- [P: working-tree diff actions](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/controllers/working_tree_diff_actions.go), [patch transforms](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/commands/patch/transform.go)
- [T: custom command contract](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Custom_Command_Keybindings.md), [resolver](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/services/custom_commands/resolver.go)
- [R: diff renderer contract](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Custom_DiffRenderers.md)
- [S: searching/filtering](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Searching.md), [range selection](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Range_Select.md)
- [U: undo limits](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Undoing.md), [Escape handling](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/pkg/gui/controllers/quit_actions.go)
- [H: stacked branches](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Stacked_Branches.md), [fixups](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/Fixup_Commits.md)
- [A: upstream architecture guide](https://github.com/jesseduffield/lazygit/blob/c5f7158154602d23b0750d4304c73e2aead8df5b/docs/dev/Codebase_Guide.md)

## Agreed product contract

1. Native Rust/GPUI interface, faithful panel structure, contextual keys and workflows. Mouse interaction is additive; no required mouse journeys.
2. Pin v0.66.0. Read compatible LazyGit config; write GUI-only preferences separately. No rewrites/creation/migration of shared LazyGit files. Config-edit shortcuts report unsupported.
3. Linux first, X11 and Wayland acceptance separate; other platforms deferred. One active repository per window, no repository tabs. Recent-repo/worktree/submodule actions switch in place like LazyGit, including the submodule parent-return stack; an explicit open-in-new-window action may remain available.
4. Incremental useful releases; parity remains the eventual goal, subject to the explicit exceptions below. Missing functionality must be visible, not silently mapped to something else.
5. First usable milestone: inspect → selectively stage/unstage files, hunks and lines → commit, with contextual navigation/help from the start.
6. Native unified diff presentation only. Old/new side-by-side comparison and configured custom diff renderers/cycling are excluded. Graphical difftool handoffs remain supported.
7. Native-first built-in workflows. Support noninteractive custom commands and external graphical tools; reject terminal-dependent workflows. No embedded or external terminal fallback. Custom output modes `none`, `log`, and `popup` remain supported; `terminal` and `logWithPty` report unsupported.
8. Preserve LazyGit's configured destructive-action confirmations. Do not add blanket mandatory prompts or promise universal undo.
9. Use existing Git/SSH configuration, helpers and agents, plus supported native prompts. No new credential vault or secrets in GUI preferences.
10. Personal/local initially. No publication/license grant implied. Automated headless checks plus the user's manual keyboard acceptance; native agent tests need separate explicit consent.
11. Newly encountered executable repository/ancestor configuration requires remembered, source-specific trust, renewed when its executable configuration changes. Ordinary settings may load without approval; explicitly chosen global configuration is trusted. This does not sandbox Git hooks, filters, helpers or tools.
12. `Ctrl+Z` reports unsupported. Applicable `q`, `Ctrl+C`, and `Q` close the current window, honoring configured quit confirmations; closing the last window exits. No launching-shell directory changes.
13. No update checks/downloads/installations; update actions/settings report unsupported. No random-tip requirement, decorative animations, terminal-artwork parity or easter eggs; practical contextual help remains required.
14. English only. Upstream translation compatibility is excluded; unsupported configured languages must be diagnosed.

## Feature inventory and proposed sequencing

The table covers product areas, including nested/menu workflows that a simple five-panel clone would miss. Stage labels refer to [the plan](LAZYGUI-PLAN.md), not implemented availability. All areas currently lack Git implementation.

| Area | Required workflows / important semantics | Native mapping | Proposed stage | Evidence |
| --- | --- | --- | --- | --- |
| Global interaction | Panel jumps `1`–`5`, main focus `0`, arrows/hjkl, tabs, help, screen modes, scroll/page/top/bottom, clipboard, quit | Context-aware native panels and menus | M1 foundation, finish M5 | K/B/D |
| Focus/selection | Context stack; selection/range anchors; enabled actions; focus restoration; sticky `v` and nonsticky Shift+arrows | View-owned focus/selection with explicit owner intentions | M1 | B/S/U |
| Status/repositories | Dashboard, all-branch logs, recent repos, repo discovery/init and nonrepo behavior | Native status and chooser; in-place transitions | M1 basic, M2/M5 finish | K/C |
| Working tree | Tracked/untracked/staged/unstaged/conflicted; directories/tree; status/text filtering; ignores/excludes; range operations; rename threshold; resets/discards | Native files tree with separate index/worktree state | M1 core, M2/M4 finish | K/P/D |
| Unified diffs | File/directory diff; two index-side panes; hunk/line/range targeting; whitespace/context; wrapping; jump/next-file; copy; edit hunk | Structured unified diff; never stage display text | M1 core, M4/M5 finish | K/P/R |
| Commit writing | Subject/body switching, previous messages, cancel/failure preservation, prefixes/wrap/sign-off, no-staged warning, hook skipping/WIP rules, amend | Native commit form; editor bridge where needed | M1 ordinary commits, M2/M4 variants | D/K/commit controllers |
| Local/remote branches | Checkout/by-name/previous/force; create/rename/delete; upstream/reset; merge/rebase; fast-forward; sort; move commits to new branch | Native lists, dialogs and workflow owners | M2/M3/M4 | K/H |
| Commit history/reflog/sub-commits | Graph/log modes, details/file exploration, range/branch selection, path/author filters, browse/copy, detach/reset, commit attributes | Native history and nested context views | M2, finish M4/M5 | K/S/U |
| Stash | All/staged/unstaged/selected variations; apply/pop/drop/rename; browse files; branch/worktree from stash | Native menus with conflict/outcome reporting | M2/M4 | K |
| Remotes/tags/forges | Remote CRUD/fetch/fork; tag create/delete/push/checkout; push/pull/force/upstream; PR URLs and browser integration | Git CLI, native prompts, graphical browser handoff | M3 | K/D |
| Stacked branches | `rebase.updateRefs` behavior; offer multi-branch push/update; recognize rewritten upstream via reflog; respect branches checked out in other worktrees | Native stack-aware workflow, not ordinary single-branch push renamed | M3/M4 | H |
| Rewrite history | Interactive rebase/todo; squash/fixup/amend/drop/reword/move/pick/revert/cherry-pick; autosquash; root/merge commits; continue/skip/abort | Native todo/message editing and Git editor bridge | M4 | K/H/A |
| Custom patches | Build from commit files/lines; apply/reverse; remove changes from a past commit/move to another commit | Native selection plus patch/rebase owner | M4 | K/P |
| Merge conflicts | Pick sides/both, navigate hunks/conflicts, edit file, conflict-resolution undo, stage resolved files, continue/abort | Native conflict editor; graphical tools allowed | M4 | K/D |
| Undo/redo | Reflog-based history restoration, not working-tree/stash/remote rollback; disabled mid-rebase | Native history action following upstream limits | M4 | U |
| Worktrees/submodules | Create/switch/remove/open worktree; initialize/update/add/remove/edit/bulk submodule; parent-return behavior | Git operations and in-place transitions with parent-return stack | M3 | K |
| Bisect/git-flow | Bisect options/state; git-flow workflows with installed dependency when required | Native menus; noninteractive external utility invocation | M4/M5 | K/controller index |
| Commands/logs/config | Custom key overrides, Go templates, menu prompts/suggestions/command-generated menus, shell commands, command log, config reload | Native forms and nonterminal output surfaces; Go template strategy pending | M0 design, M5 full | T/C/B |
| Renderer/OS integration | `stdinFilter`, `extDiff`, `rawGit`; tool/editor/clipboard/link commands; platform/key-name normalization | Native unified display; custom renderers excluded; graphical tool handoffs allowed | M0 decision, M5 finish | R/D |
| Terminal/product miscellany | Suspend, parent-shell cwd changes, subprocess-return prompt, update checks, tips, snake/explosion | Explicit exceptions: suspend, cwd behavior, updates and decorative extras | Exception handling in owning stage | K/D/controllers |

## Keyboard compatibility findings

### Not a flat keymap

The native action resolver needs a `(context, mode, selection, availability)` contract. Examples:

| Input | Meaning in relevant contexts |
| --- | --- |
| `Space` | Files stage/unstage; unstaged diff stage; staged diff unstage; stash apply; branch/tag/commit checkout; commit-file diff custom patch inclusion; conflict pick hunk |
| `Tab` | Side-panel navigation, staged/unstaged pane switch, commit subject/body switch; **not** generic widget tab traversal everywhere |
| `Ctrl+S` | Global history-filter options, but commit body confirmation (an alternate to `Ctrl+Enter`) |
| `Ctrl+O` | Contextual copy elsewhere, but commit-menu invocation in subject/body inputs |
| `n` / `N` | New entity vs next/previous search match vs next/previous diff file, depending on context |
| `d` | Unstage on staged diff; confirmed destructive discard on unstaged diff; deletion/drop in lists |
| `z` | Global reflog undo, but local conflict-resolution undo in the conflict context |
| `Esc` | Cancel selection/filter first, return from nested view, cancel mode, return to parent repository, optionally quit at top level |

Custom commands override built-ins **in the same context**, but a context-specific built-in beats a global custom binding. Search/filter prompt activation restricts dispatch to that prompt. Guards can disable a command, explain why, or permit further dispatch; unavailable actions aren't just absent handlers. Help must be derived from the same resolved actions, including disabled explanations.

The key grammar supports scalar/list bindings, aliases, modifier normalization, printable uppercase/punctuation, special keys, `<disabled>`, and legacy alternate-key merging. GPUI's spelling is not LazyGit's spelling: compile to a normalized representation, do not string-substitute and hope. The upstream Linux export includes legacy alternate fields, so capture their merge semantics as well.

GPUI Kit 0.7.1 exposes GPUI actions/focus and headless input dispatch. Its base input has existing editing bindings (e.g. Tab indentation and arrow movement); input-specific LazyGit actions need deliberate precedence. Inspecting those APIs establishes a plausible route, **not proof** that current inputs support every shortcut or IME interaction. Test subject Enter versus body newline/confirmation, arrow history recall, Tab switching, paste, and composition before building all dialogs.

### Stable target identities

Diff cursor/selection must refer to raw patch identities, not screen row indices. Wrapped lines, syntax highlighting, diff headers and changed context size are presentation. Resolve the target's file, index side, old/new line and addition/deletion identity against a known snapshot.

### Staging semantics confirmed from source and fixtures

- Staged changes stay in the secondary/lower pane, even when they are the only changes; unstaged changes occupy the primary/upper pane. Empty panes disappear; `0` and scroll actions follow the surviving pane.
- `a` toggles line versus hunk selection; default `gui.useHunkModeInDiffView` is true.
- Partial stage uses a forward patch against the index; partial unstage reverses a staged patch against the index. Unstaged discard reverses into the worktree and follows the configured confirmation.
- Complete selection of a deleted/added file uses file-stage/unstage semantics: do not accidentally turn deletion into a tracked empty file, or unstaging an addition into an empty tracked file.
- Zero diff context explicitly blocks line staging/discard in upstream.
- Selection moves to the next surviving change; when the acted side disappears, focus/selection follows the other pane.
- Rapid `Space Space` presses are held until refresh and diff target reconciliation complete; **neither drop the second press nor apply it to stale rows**.
- Commit shortcuts in the main view operate only when its source is the working tree, not while browsing a commit diff.

Reviewed fixtures include `file/staged_changes_in_lower_pane.go`, `main_view/stage_hunks_with_rapid_keypresses.go`, `main_view/select_next_change_after_staging.go`, `main_view/stage_deleted_file.go`, `main_view/selection_commands_only_where_they_apply.go`, and `file/remember_commit_message_after_fail.go`. The durable test-file index links all fixtures. These are design evidence, not tests that have passed for LazyGUI.

## Configuration compatibility findings

### Loading must match semantics, not just names

- Start from platform defaults. Global files are applied in order. Later files override ordinary supplied fields; **custom commands accumulate with the new file's commands before existing ones**. This is not a generic deep merge with universal array replacement.
- Config lookup includes `CONFIG_DIR`, legacy `jesseduffield/lazygit`, XDG search paths, and `lazygit/config.yml`; `LG_CONFIG_FILE`/CLI custom-file behavior is also relevant. Separate LazyGUI preference paths from these sources.
- Repository configuration in this release reads `.lazygit.yml` in **ancestor directories above the repository** and `<resolved repository git dir>/lazygit.yml`. The checkout-root `.lazygit.yml` is explicitly a TODO, not implemented. Do not invent support for it and call that identical.
- LazyGit can migrate/create files. LazyGUI's read-only promise intentionally differs: migrate in memory only, surface diagnostics, never write shared files.
- Config reload must update active behavior or clearly request restart where unavoidable. Invalid changes should not half-apply a new keymap.
- Unknown/unsupported settings need source-aware diagnostics. Mapping each of the 362 schema paths remains work; no blanket configuration-compatibility claim is warranted.

### Suggested mapping categories (not yet per-field classifications)

1. **Exact behavior:** keybindings, warning flags, Git operation settings, refresh/fetch policy, commit rules, sorting/filtering.
2. **Native semantic mapping:** colors/theme, panel proportions/order, wrapping, scroll margins, status/graph/icon presentation; terminal-cell-specific values need documented translations.
3. **Supported external execution:** noninteractive command output, browser/editor/difftool and configured shell/clipboard helpers when graphical/nonterminal.
4. **Explicit exception:** terminal subprocess output and terminal editor interaction; no side-by-side renderer obligation.
5. **Approved policies:** config editing, PTY output, custom renderers, suspend, updates and decorative extras are unsupported; quit keys close the current window without parent-shell cwd changes. Executable repo/ancestor config requires remembered source-specific trust. English only.

### Custom commands are a compatibility subsystem

The source requires Go `text/template` semantics, not only `{{field}}` substitutions: pipelines/functions (`quote`, `runCommand`), expressions/conditionals, selected object models, commit-range endpoints, `.Form` values and nested command menus. Inputs can have presets or command-generated suggestions; prompt types are input/confirm/menu/menuFromCommand, including conditional prompts and regex-derived values/labels. Fields beyond the cheatsheet table (e.g. `commandMenu`) must be included.

Command output modes: `none`, `log`, `popup`, `terminal`, `logWithPty`. The first three map to supported nonterminal execution. Both `terminal` and `logWithPty` are explicitly excluded. Although PTY output needn't be interactive, the user chose not to support it; do not silently substitute ordinary log execution.

A Rust Go-template-compatible implementation versus a narrow pinned Go helper is an implementation tradeoff to validate. Do not translate templates into a different language or claim a subset is complete compatibility. Config can execute code while resolving templates/suggestions, before a final command runs; repository-config trust therefore matters.

## Approved policy resolutions and compatibility exceptions

Resolved with the user through one-at-a-time Q&A after the initial investigation. These supersede the original recommendations; none imply implementation or native acceptance.

| Policy | User decision | Compatibility consequence |
| --- | --- | --- |
| Shared-config editing | Status `e` and global `Alt+Shift+C` report unsupported; no inspector/editor required | Keep key recognition and clear feedback; never edit shared files |
| Repository transitions | Switch in place like LazyGit, one active repo/window, no tabs | Preserve recent-repo/worktree switching and submodule Enter/Esc return stack; refresh state bound to the new identity |
| Executable repository/ancestor config | Remembered, source-specific trust, renewed on executable configuration changes; explicitly chosen global config trusted | Ordinary settings may load, but template/suggestion/command execution waits for approval; intentional security prompt |
| Terminal lifecycle | `Ctrl+Z` unsupported; applicable `q`/`Ctrl+C`/`Q` close current window, last-window close exits; honor quit confirmations | No suspend/minimize substitution and no parent-shell cwd changes |
| Custom diff renderers | Native unified renderer only; configured custom renderers and cycling unsupported | Diagnose settings/actions; graphical difftool handoffs remain allowed |
| PTY output | `logWithPty` unsupported, alongside `terminal`; retain `none`/`log`/`popup` | No PTY execution and no silent ordinary-log fallback |
| Updates | Excluded for now | No checks/downloads/installations; actions/settings report unsupported |
| Decorative extras | Excluded; practical help retained | No snake/explosion/random-tip or exact terminal-artwork requirement |
| Languages | English only | Upstream translations excluded; unsupported configured languages diagnosed |

Previously agreed exceptions remain: no terminal UI/editor/tool fallback, no arbitrary interactive terminal subprocesses, and no old/new side-by-side diffs. Native replacements may differ from terminal tools while preserving built-in workflow intentions. GUI preferences remain separate from read-only shared LazyGit configuration.

The identified policy conflicts are closed. Detailed action semantics, configuration-field mapping, helper protocol design and feasibility tests remain technical work—not permission to broaden these exceptions. Executable-config trust does not sandbox Git hooks, filters, agents, helpers or arbitrary tool commands; their execution/security boundaries must remain explicit. Unknown tool interaction cannot reliably be detected from a command string.

## Technical feasibility and risk summary

Use installed Git as authoritative engine. Upstream itself invokes the Git binary; no stable machine-readable LazyGit engine API was established by this investigation. Its Go GUI/controllers contain substantial workflow policy, so a Rust GUI cannot get parity just by calling `git` or importing one library. Use source/fixtures as behavioral reference; do not inherit upstream's all-purpose GUI/common structs against this project's ownership rules.

Highest risks, in order:

1. Correct partial patches and target preservation during rapid asynchronous actions.
2. Native focus/input dispatch, including popup suppression and IME/paste safety.
3. Go-template/config merge and command-output compatibility.
4. Child process lifecycle: aborting an async task does not necessarily terminate/reap Git or undo a completed write.
5. Worktrees/external modifications and shared refs/index contention across windows.
6. Interactive Git editor/sequence-editor and askpass bridges without any terminal fallback.
7. Scope underestimated by advanced history/stacked-branch/custom-patch workflows.

Credential prompts are protocol-specific: Git HTTPS askpass, OpenSSH askpass constraints, and GPG/pinentry are not a universal prompt stream. Signing should normally remain with existing agents/graphical pinentry; do not promise that Git askpass solves GPG. No live credential experiments were performed.

## Licensing and provenance

Pinned LazyGit is MIT (copyright Jesse Duffield, 2018). Extracted upstream action descriptions/schema descriptions and defaults carry its unchanged notice in [upstream-LICENSE.txt](research/lazygit-v0.66.0/upstream-LICENSE.txt). This does **not** license the LazyGUI template or grant publication rights for this project. Dependency and any future copied code/assets require separate review before distribution. No upstream production code has been incorporated into `src/`.

## Next deliverable

[LAZYGUI-PLAN.md](LAZYGUI-PLAN.md) defines proposed ownership, prerequisites, M0 spikes, first-milestone acceptance and the later parity audit. No implementation is authorized merely by this document, and no proposed exception is accepted merely by being recommended here.
