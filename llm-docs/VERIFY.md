# Verification and safety

## Automated command

Run `./scripts/check.sh` (it changes to the repository root, so works from any directory). Its exact commands are:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

Building is not launching. [M1-RESULTS.md](M1-RESULTS.md#final-verification-and-resolved-defects) records the actual snapshot's counts/output; [its matrix](M1-RESULTS.md#automated-acceptance-matrix) links exact suites/test names. Current product tests combine installed Git in disposable temporary repositories, actual production constructors/Kit Root controls and controlled transports/fake time. Labels/compilation alone are not acceptance.

Fixtures isolate HOME/XDG/global/system Git configuration, inherited repository/index environment, hooks/signing/credential programs. Only fixture-owned hooks/children run; no user repository commits or accounts. Installed Git 2.56.0 is the validated baseline and enforced conservative floor. Owner tests cross production workflows, not cfg(test) copies. Gate/retention/shutdown tests hold real or controlled requests through cleanup/delivery.

M0's historical 45 Rust tests (25 new plus 20 starter) are not today's total; starter suites were removed with the demo. Historical M0 results/investigation remain unchanged.

Optional separate template feasibility oracle, not part of product build/CI:

```sh
bash scripts/check-m0-templates.sh
```

Nine historical Go oracle cases establish a possible route only. No `templatesGo` production helper, Go-template/custom-command parity or production dependency is approved. Do not record an oracle rerun unless actually run.

Linux CI invokes the check script without dev.sh/cargo run/native desktop/provider access. Hosted CI execution is not established by local passing checks. Original template generator smoke results are historical; package/library/title changes need a fresh separate scaffolding audit before repeating those claims.

## Native safety gate

Agents need separate explicit consent before GUI launch, desktop automation or real keyring/provider access. Require an already unlocked dedicated test session; stop if locked/unknown, never unlock/bypass it. Use disposable repository snapshots and worktree-local GUI config. XDG config isolation does not isolate shared agents/wallets; use a disposable OS user or demonstrably private providers. No real accounts/secrets in logs/screenshots; stop only owned processes.

## User-reported native verification

On 2026-10-09 the user confirmed that the native **inspect/stage/unstage/commit core loop** worked. Environment, exact revision and detailed cases were not supplied. Record this as core-loop confirmation only; do not infer IME, accessibility, edge-case or both X11/Wayland verification. See [the acceptance record](M1-RESULTS.md#user-reported-native-acceptance--2026-10-09). Agent native-launch/provider permission is unchanged.

## Disposable human acceptance checklist — pending

The detailed checklist below remains pending; core-loop confirmation does not check off unreported individual journeys. The human may run the documented CLI in an unlocked isolated desktop and compare stock LazyGit v0.66.0 against disposable **local** fixtures; record exact focus/selection/index/worktree/message results, not screenshot resemblance.

- [ ] Open cwd/`--path`, nested/unborn/detached/bare/linked/nonrepo; switch in place, no stale rows or writes in unsupported contexts.
- [ ] Five-panel navigation, Files `2 → j/k/arrows → Enter/0 → Esc`, directories, tree/flat, text/status filters, visible ranges, help/copy and resizing/scroll/layout.
- [ ] Mixed staged/unstaged: Tab side changes, `a` line/hunk, `v`/Shift+arrows, partial Space, rapid Space Space/interleaved navigation; last-change focus follows surviving pane.
- [ ] New/delete/rename/binary/symlink/mode/no-final-newline/unusual paths: inspect independent index and worktree bytes, explicit whole-file fallback.
- [ ] Commit subject Enter, Tab body/newline, Ctrl+S/Ctrl+Enter; cancel/reopen, fixture failing hook/corrected deliberate retry, no-stage warning/skip, signoff/wrapping/prefix and full recall.
- [ ] Real IME candidate Enter/Escape/Tab, physical layouts, selection/edit/paste, clipboard options, popup suppression/focus restoration and keyboard-only operation.
- [ ] External file/index changes and config reload/invalid rollback; unavailable setting/action feedback.
- [ ] Close during a disposable local child/hook and simultaneous window closes: UI remains responsive, owned processes settle; no replay/false success.
- [ ] X11/Wayland, assistive technology/accessibility and graphical driver behavior.

Native core-loop acceptance is user-confirmed; full checklist acceptance is not claimed. Real signing/SSH/askpass/editor/provider integration, network M3 workflows, packaging/performance budgets and Windows/macOS remain separate unverified scope. Ordinary production Git hooks/signing configuration is preserved, but that does not establish live prompt compatibility.
