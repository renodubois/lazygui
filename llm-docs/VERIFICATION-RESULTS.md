# Recorded verification

Local implementation/verification on 2026-10-08, Linux x86_64. Rust 1.97.0, Cargo 1.97.0, GPUI Kit 0.7.1, cargo-generate 0.25.0. This starter follows Hamlet's ownership patterns at commit a94c3272dcbac133d8374f84da0fd73901b72e5a; no Hamlet crate/source path is a build dependency.

## Original verification (before Python removal)

| Scenario | Checks / result |
| --- | --- |
| Template itself | scripts/check.sh: formatting, strict all-target Clippy, 20 tests, locked build |
| cargo-generate project named smoke-gui | Same locked checks and 20 tests; root manifest/lock renamed, fresh Git, no profile or hook directory |
| Plain tar copy renamed copied-gui | Same locked checks and 20 tests |
| Extension recipe dry-run | Disposable generated copy: added settings view, retained connection owner and independent typed build-version connector; 22 tests and all checks passed |
| Domain replacement dry-run | Disposable generated copy: removed catalog/connectors/storage/runtime, pruned dependencies, added tally state/view; 2 tests and all checks passed |
| Scripts | bash -n for shell scripts; dev.sh --help; invalid rename rejected without changes |
| Dependency isolation | cargo metadata lists only registry dependencies, no Hamlet/protocol/server paths |

The template was compiled initially into its own target directory. Generated/copy/recipe dry-runs reused only those compilation artifacts for speed; each had independent source, package identity and isolated temporary fixtures. No Hamlet target directory, server or database was used.

Tests exercise pure request/selection transitions, stale/duplicate completions, cancel-on-drop and controlled deadlines, real Kit controls/production constructors, screen recreation during pending work, real loopback query encoding/response validation/redirect rejection/transport failure/timeout/body size, and temporary-file ordered persistence/failures.

The first headless view journeys exposed missing test registration on custom status/detail nodes. Adding their semantic accessibility labels and test_support registration fixed both failures; regression journeys assert selection, empty-state and error labels. Kit controls already supplied native registration.

## Python-free scaffolding verification

After replacing the Python workflow, `./scripts/check.sh` passed formatting, strict all-target Clippy, all 20 tests and the locked build. `./scripts/test-template.sh` passed the same checks for generated projects named smoke-gui, smoke_gui (with --force), and gpui-template. Exact comparisons confirmed Cargo.toml/Cargo.lock changed only the root name, dependency versions were retained, Rust source was unchanged, and generated projects had fresh Git repositories without profiles, hooks or template test scripts. Shell syntax checks and dev.sh --help passed. No native GUI was launched.

## Deliberate implementation choices

- The example owner is a non-rendering GPUI Entity rather than Hamlet's Rc-backed chat handle; GPUI observation supplies independent invalidations.
- Theme setup uses Kit's built-in dark tokens, not a copied full Hamlet palette.
- The runnable source tree contains no Liquid placeholders. Originally, generation used a Python rename script; this has been replaced by a reviewed sed pre-hook, Cargo-file-only Liquid substitutions and a cleanup post-hook. No Python is required. Generation requires command approval; it never builds or launches the app.
- Authentication, credentials, streaming and owned backend remain documented optional recipes, not implemented base modules.
- License remains unset by owner choice; no public distribution grant is implied.

## LazyGUI M0 verification

On the same Linux/Rust/Kit baseline, `./scripts/check.sh` passed format, strict all-target Clippy, **45 Rust tests** (25 new M0 + the existing 20 starter tests), and locked build. `bash scripts/check-m0-templates.sh` separately passed **9** standard-library Go-template oracle cases with Go 1.27.1 and module/toolchain fetching disabled. Git 2.56.0 was used only against disposable isolated fixtures. Headless Root input and window-close tests ran without native desktop launch. Fake prompt IPC exercised permissions/authentication/replay/bounds/abort/cancellation/cleanup; no real helper/provider was installed or called.

See [M0-RESULTS.md](M0-RESULTS.md) for decisions, exact suites and explicit remaining limitations. The executable is still the catalog starter; M0 does not establish M1 usability, full configuration/template coverage or native parity. Template generation smoke tests were not rerun for this product-feasibility change.

## Not established

No native GUI was launched, no desktop input/automation performed, and no keyring or live external account accessed. Native startup, resizing, physical keyboard/IME, accessibility, graphical driver behavior, packaging, other operating systems and GitHub-hosted CI execution remain unverified. Local passing checks are not claims of these acceptances.

Disposable recipe additions were validation probes, not production integrations or additions to the base template. No remote repository was created/pushed and no commit was made.
