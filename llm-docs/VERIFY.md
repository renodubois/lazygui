# Verification and safety

## Automated

From the repository root: `./scripts/check.sh`, which runs cargo fmt --check, cargo clippy --locked --all-targets -- -D warnings, cargo test --locked and cargo build --locked. Building is not launching. Test fixtures use controlled schedulers/outcomes, ephemeral loopback servers and temporary files; tests do not contact real accounts or user preference paths.

M0 adds isolated installed-Git fixtures and Linux owned subprocess tests to the same check command (45 total Rust tests). Fixtures clear inherited environment, use explicit temporary HOME/XDG/global/system Git configuration, disable inherited hooks/signing/credential helpers, and never contact real repositories/providers. See [M0-RESULTS.md](M0-RESULTS.md) for the exact evidence and conservative support floor (Git 2.56.0 validated).

The optional, separate template feasibility oracle requires a locally installed Go toolchain, fetches no modules, invokes no commands/providers, and is not part of application build or CI:

```sh
bash scripts/check-m0-templates.sh
```

Its nine cases establish a Go-template implementation route, not a shipped helper or full compatibility. No native GUI, real editor/askpass provider, SSH agent or signing program is exercised.

The Linux CI workflow installs native build prerequisites and runs the same script. It does not invoke dev.sh, cargo run, desktop automation or credential providers.

Generator smoke test (after reviewing the hook):

```sh
destination=$(mktemp -d)
cargo generate --path "$PWD" --name smoke-gui --destination "$destination"
(cd "$destination/smoke-gui" && ./scripts/check.sh)
```

The original template also provides `./scripts/test-template.sh`, which automates generation and the same checks using reviewed local sed hooks for kebab-case, snake_case and unchanged package names; it is omitted from generated projects. It reuses only the template's compilation artifacts.

Check that the new package/lock entry/title/config namespace match smoke-gui; no Hamlet path dependencies, profiles, hook directory or source Git history should remain. The script compares both Cargo files against root-name-only replacements and confirms Rust sources are untouched. Plain copy/manual rename should pass the same locked checks. See [VERIFICATION-RESULTS.md](VERIFICATION-RESULTS.md) for actual recorded checks; claims here describe intended coverage, not native acceptance.

## Native safety gate

Agents need separate explicit consent before launching the GUI, desktop automation or real keyring access. Require an already unlocked dedicated test workspace/session; stop if lock state is locked or unknown, and never bypass/unlock the desktop. Use worktree-local config and disposable data. If authentication is later added, XDG_CONFIG_HOME alone does not isolate shared credential providers: use a disposable OS user or demonstrably private provider/session, never the ordinary wallet for drills. Do not log/screenshot secrets.

## Human native checklist (not yet claimed verified)

With an appropriate unlocked graphical session, run with worktree-local configuration and default memory data:
- Window opens/resizes and list/detail layout remains usable.
- Selecting a row updates details; keyboard focus traverses search/actions correctly.
- Enter submits, empty search restores all records, no matches displays an empty state, Reload is responsive.
- IME candidate Enter does not accidentally submit before commit; input selection/copy/paste work.
- Labels/focus/actions are usable with assistive technology.
- Preference write failures are visible; restarting restores only the submitted query.
- A consented owned loopback server outage produces failure while old data remains; recovery needs a deliberate Reload, not hidden retry.

Native GUI, IME/accessibility, packaging, Windows/macOS and external live-provider acceptance remain separate and unverified until recorded. Only owned processes may be stopped; never kill another worktree's process to free a port.
