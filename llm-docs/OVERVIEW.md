# LazyGUI — M1 inspect/stage/commit implementation

LazyGUI is a Linux Rust 2024/GPUI Kit Git client using the installed Git CLI. Startup now opens a repository, not the former catalog/HTTP demonstration. Rust 1.97.0 and GPUI Kit 0.7.1 remain pinned. Git **2.56.0 or newer** is an enforced conservative tested floor, not a claim that every flag requires that version.

**Status:** M0 feasibility and M1 automated acceptance passed (**188 tests**). The user confirmed the native core inspect/stage/unstage/commit loop on 2026-10-09; extended native-checklist acceptance remains pending. No full LazyGit/configuration parity is claimed. See [M1-RESULTS.md](M1-RESULTS.md) for exact test evidence, decisions and gaps.

## Build and human development

Install Rust/rustup, a C/C++ compiler, pkg-config, fontconfig/freetype, xkbcommon, Wayland/X11 development libraries, D-Bus libraries and Vulkan loader/driver; `.github/workflows/check.yml` lists the Ubuntu baseline. Linux is the validation target; packaging and Windows/macOS are unverified.

```sh
./scripts/check.sh # format, strict Clippy, locked tests/build; never launches GUI
# Human-run only, with disposable data and an unlocked isolated desktop:
XDG_CONFIG_HOME="$PWD/.env.dev-config" cargo run --locked -- --path /absolute/disposable/repo
```

Human build/watch/restart uses `./dev.sh` (Bash, Rust, Watchexec and setsid), with worktree-local `.env.dev-config`; do not copy another worktree's profile. Native launch/automation and real credential-provider access require separate explicit consent. Configuration isolation is not credential-provider isolation.

## Opening and configuration

Supported arguments are `--path PATH` and `--use-config-file FILE[,FILE]`, also `--name=value` forms. No arguments opens cwd; relative repository paths resolve against startup cwd, preserving Linux path bytes. Unknown arguments/missing values exit with usage/error (status 2). Repository errors produce a safe window state; older Git is rejected before installing feature owners.

One active repository per window; switching replaces it in place and refuses pending/queued mutations. Discovery binds canonical worktree, Git directory and common directory, including linked-worktree `.git` files. Unborn/detached repositories are usable; bare repositories are read-only history/HEAD inspection. Alternate inherited Git indices/repository-selection environment overrides are deliberately not adopted.

Shared LazyGit YAML is **read-only**:
- CLI config list overrides `LG_CONFIG_FILE`; list order is preserved, explicit missing files fail.
- Otherwise `CONFIG_DIR/config.yml`, or XDG/HOME lookup, searches legacy `jesseduffield/lazygit` before `lazygit`.
- Ancestor `.lazygit.yml` sources above the root load outermost first, then `<git-dir>/lazygit.yml`. Canonical aliases are deduplicated.
- Relative global sources are anchored to invocation cwd before `--path` processing and remain fixed across switches/reloads. No shared file/directory creation, migration or editing.
- Supplied-field merge, bindings/legacy alternates and source-aware diagnostics are retained. Reload validates the whole candidate before replacing active policies.

Supported settings are narrower than parsed settings: see [M1 configuration accounting](M1-RESULTS.md#configuration-accounting). Unavailable bindings/settings are diagnosed rather than granting their workflows. `customCommands` and Go-template execution are unavailable; a production Go helper is not approved. Auto-fetch/network workflows wait for M3 and are diagnosed, never silently launched.

## Implemented loop

Five native side panels show status, files, current branch, read-only HEAD commits, and explicitly unavailable stash; worktrees/submodules/remotes/tags/reflog tabs are labeled unavailable, not fake rows.

Files support tree/flat, substring/fuzzy text and exact status filters, visible-only ranges, panel navigation/help/copy. Two **unified** unstaged/index panes can be stacked or arranged alongside each other (not old/new side-by-side diff). Hunk/line/range staging and unstaging use canonical byte snapshots. Repeated keys wait for refresh and remap surviving identities; stale external targets are rejected. Unsafe partial formats have explicit whole-file fallback.

Commit uses real subject/body Kit controls with text/IME/popup suppression, retained drafts, full previous-message recall, clipboard options, no-staged confirmation, wrapping/signoff, configured branch prefixes and literal hook-skip prefix policy. Git receives the message via stdin; hooks are not bypassed unless that explicit prefix selects `--no-verify`. Failure/cancel/uncertain outcomes are reconciled without automatic write replay.

Automatic refresh is polling, not a filesystem watcher: coalesced ticks do not cancel slow reads or overtake queued actions. Explicit refresh and post-write reconciliation retain usable data on failure. Closing waits asynchronously for owned process/workflow settlement; it does not block the GPUI thread.

## GUI profile: storage capability versus connected behavior

GUI files are separate: `$XDG_CONFIG_HOME/lazygui/{preferences,trust}.json`, falling back to `$HOME/.config/lazygui/`. Startup reads defaults/existing preferences and trust without creating files; window size is consumed. One shared ordered writer supports preferences and fingerprint-only trust approval/revocation and the shell flushes it on close.

**Current runtime does not enqueue preference saves or trust approvals/revocations.** Resizes/recent paths are not automatically persisted; loaded trust is not connected to an executable-config approval UI. Custom execution stays disabled regardless. Atomic replacement is neither multi-process locking nor crash-durable fsync. No drafts, commands or credentials belong in these files.

## Guides and historical evidence

- [ARCHITECTURE.md](ARCHITECTURE.md), [EXTENDING.md](EXTENDING.md), [CUSTOMIZE.md](CUSTOMIZE.md), [VERIFY.md](VERIFY.md).
- [LAZYGUI-PLAN.md](LAZYGUI-PLAN.md): milestone contract and later scope.
- [M1-RESULTS.md](M1-RESULTS.md): current automated matrix and exact check summary.
- [M0-RESULTS.md](M0-RESULTS.md), [LAZYGIT-INVESTIGATION.md](LAZYGIT-INVESTIGATION.md), [research](research/lazygit-v0.66.0/OVERVIEW.md): historical pinned v0.66.0 evidence, not current runtime inventories.
- [VERIFICATION-RESULTS.md](VERIFICATION-RESULTS.md): preserved original/template/M0 verification and current M1 pointer.

README/human documentation is agent-read-only. Generated documentation belongs here. Licensing remains undecided; no publication/license grant is implied.
