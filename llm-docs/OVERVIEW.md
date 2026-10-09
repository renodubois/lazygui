# GPUI starter

A standalone Rust 2024 GUI project based on Hamlet's ownership patterns, not its chat domain. GPUI Kit is pinned to 0.7.1; Rust to 1.97.0. One window retains a non-rendering catalog feature owner, whose data is displayed by composed list/detail/search views. No Hamlet checkout, server, database, keyring, authentication, or network account is required.

## Create a project

Rust has no built-in equivalent of `cargo new --template`. Use [cargo-generate](https://cargo-generate.github.io/cargo-generate/) for repeatable local/Git templates, or copy the repository and rename it. `cargo-generate` is a separate scaffolding tool, not a runtime dependency.

Recommended, from the directory that will contain your new project:

```sh
# Once, when you want to install the scaffolding tool:
cargo install cargo-generate --locked --version 0.25.0

cargo generate --path ~/projects/gpui-template --name my-gui
cd my-gui
./scripts/check.sh
```

Generation asks permission to run a local `sed` command. Review `template/rename.rhai` before approving. The pre-hook inserts `{{project-name}}` into the root Cargo package and its matching lockfile entry in the staging copy; cargo-generate expands only Cargo.toml/Cargo.lock. The post-hook removes the hook directory. Generation requires sed (Linux), but no Python; Rust alone is enough for building. It never builds, launches a GUI or contacts a provider. Rust source, YAML and docs are excluded from Liquid processing, so ordinary braces remain intact. Do not use `--allow-commands` with unknown templates.

Plain-copy alternative (choose a new, nonexistent destination):

```sh
mkdir ~/projects/my-gui
tar -C ~/projects/gpui-template --exclude=.git --exclude=target --exclude='.env.*' -cf - . \
  | tar -C ~/projects/my-gui -xf -
cd ~/projects/my-gui
# Rename only the exact root package entry in both files:
sed -i 's/^name = "gpui-template"$/name = "my-gui"/' Cargo.toml Cargo.lock
rm -rf template cargo-generate.toml scripts/test-template.sh
git init -b main
./scripts/check.sh
```

Do not copy target/, user profiles, secrets or source Git history. Cargo-generate normalizes names to kebab-case by default; use --force to preserve snake_case. For manual copies, use lowercase kebab/snake package names. Cargo package name determines the binary, window title and config namespace automatically. Generated projects keep locked dependencies and no template hook directory. Nothing is published or configured on GitHub.

## Linux setup and run

Install Rust/rustup, a C/C++ compiler, pkg-config, fontconfig/freetype, xkbcommon, Wayland/X11 development libraries, D-Bus libraries and a Vulkan loader/driver. See the package list in `.github/workflows/check.yml` for an Ubuntu baseline. X11/Wayland GUI use needs an appropriate graphical session/driver; headless tests do not need a desktop or Secret Service. Linux is the current validation target; Windows/macOS and packaging are not claimed supported.

```sh
./scripts/check.sh   # format, strict clippy, tests, build; no GUI launch
# Human-run native development only:
XDG_CONFIG_HOME="$PWD/.env.dev-config" cargo run --locked
```

The default is deterministic in-memory example data. Search submits with Enter or the Search button; Reload repeats the submitted query. Selection survives reload by identity. Loading, no matches and failures are visible; failed reads preserve prior data. Reads can be superseded safely.

For build/watch/restart, install Watchexec (`cargo install watchexec-cli --locked`) and run `./dev.sh`. This Linux script requires Bash, Rust, Watchexec and setsid. It watches this crate only, retains the running client on failed builds, uses a worktree-local profile/native target, and stops only its owned processes. Closing the window does not stop the watcher. Restarts discard in-memory state.

## Optional loopback HTTP example

Set `CATALOG_URL=http://127.0.0.1:<your-owned-port>/records` explicitly to use the read-only HTTP adapter. Only literal loopback IP addresses, HTTP and the /records path are accepted; localhost/DNS, credentials, query and fragment in configuration are rejected. Invalid configuration stops startup instead of falling back.

The adapter sends `GET /records?q=<encoded-query>` and expects a 200 JSON collection:

```json
{"items":[{"id":"one","title":"Example","description":"Details"}]}
```

IDs must be unique/nonempty and titles nonempty. The adapter does not follow redirects, inherits no proxy, caps response bodies at 1 MiB and bounds requests with transport/feature deadlines. No local server is bundled or automatically launched. Automated HTTP tests bind disposable ephemeral loopback ports. Production providers need their own connector/policies, not this demo endpoint validator.

## Preferences

Submitted search query only is persisted in `$XDG_CONFIG_HOME/<cargo-package>/preferences.json`, falling back to `$HOME/.config/<cargo-package>/preferences.json`. Config errors and save failures are surfaced without preventing catalog use. No records, passwords or tokens are stored. One ordered file worker operates per root; each running instance needs its own profile. Small startup reads occur before the GUI event loop; subsequent writes happen off the GPUI thread. Atomic rename is not a claim of crash-durable fsync.

## LazyGUI — M0 complete, usable Git client not yet implemented

The executable remains the catalog starter. Headless M0 capability prototypes are compiled in the library, separate from startup. The LazyGit-compatible product is documented separately:

- [LAZYGIT-INVESTIGATION.md](LAZYGIT-INVESTIGATION.md): pinned v0.66.0 evidence, agreed scope, features/bindings/configuration, and approved policy exceptions.
- [LAZYGUI-PLAN.md](LAZYGUI-PLAN.md): ownership, completed M0 gate, incremental milestones, and acceptance gates.
- [M0-RESULTS.md](M0-RESULTS.md): 25 new Rust spike tests, 9 Go-template oracle cases, implementation decisions, interfaces and remaining limitations; no native/parity claim.
- [Research artifacts](research/lazygit-v0.66.0/OVERVIEW.md): reproducible source-linked indexes and Linux defaults; no parity claim.

## Guides

- [ARCHITECTURE.md](ARCHITECTURE.md): module interfaces, ownership, dependencies and lifetimes.
- [EXTENDING.md](EXTENDING.md): where new views/features/connectors/storage/tests go and how to wire them.
- [CUSTOMIZE.md](CUSTOMIZE.md): rename/rebrand and remove the example completely.
- [VERIFY.md](VERIFY.md): automated checks, generator smoke tests and native safety.
- [VERIFICATION-RESULTS.md](VERIFICATION-RESULTS.md): actual local check outcomes and explicit native limitations.
- Optional [authentication](recipes/AUTHENTICATION.md), [streaming](recipes/STREAMING.md) and [owned backend](recipes/OWNED-BACKEND.md) recipes.

This repository is intentionally unlicensed for now. Decide licensing/attribution before distribution. README files are reserved for human authors; generated instructions live here.
