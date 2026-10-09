# Customize or replace the example

Paths/commands are relative to the repository root.

## Naming and branding

Prefer `cargo generate --path <template-path> --name my-project`. For a plain copy, replace the exact `name = "gpui-template"` line in Cargo.toml/Cargo.lock with your package name (see [OVERVIEW.md](OVERVIEW.md)); do not rename dependency packages. The binary, native title, heading and configuration namespace follow env!("CARGO_PKG_NAME"); dev.sh asks Cargo to parse the manifest for the binary name. Keep Cargo.lock committed.

Edit theme.rs for product visual policy. Default Kit dark tokens/assets are used; no Hamlet branding, icons, passwords, database or configuration are copied. Add assets intentionally and record ownership/licenses. Decide a license before publishing; no public grant is implied by this local starter.

## Remove catalog completely

1. Design a focused feature and its typed connector/data interface; use [EXTENDING.md](EXTENDING.md).
2. Add src/<feature>/ and its owner-local tests, declare it in main.rs, and create its views under views/<name>/.
3. Change app_shell::open/constructor to accept the new dependencies and retain the new feature; preserve a single result consumer and parent child-composition.
4. Change main.rs dependency construction/default configuration. Remove CATALOG_URL selection if unused.
5. Replace Config.query/persistence policy (or remove persistence entirely). Adapt explicit-path tests; do not reuse the query field as a generic data store.
6. Remove src/catalog/, src/connectors/catalog/, src/views/catalog/ and catalog fixtures once no callers remain. Update connectors/mod.rs, views/mod.rs, test_support/mod.rs, view journeys, and imports/declarations in main.rs.
7. Remove unused reqwest/serde/Tokio/etc dependencies only after checking remaining modules. Run cargo check to refresh Cargo.lock; then run ./scripts/check.sh with --locked checks.
8. Update OVERVIEW/ARCHITECTURE/EXTENDING/VERIFY and optional recipes to describe actual behavior. Search for catalog/CATALOG_URL/query/sample IDs and review remaining references.
9. Keep agent guidance/check scripts and isolated native development unless the new app intentionally replaces them.

Do not delete only the behavior directory while views, connectors or test fixtures still refer to it. Do not replace several feature owners with one all-purpose AppState.

## Generation implementation

cargo-generate.toml limits Liquid expansion to Cargo.toml/Cargo.lock. The pre-hook uses a fixed, locally approved sed command to replace the exact root package name with `{{project-name}}` in the staging copy. Cargo-generate then substitutes the project name, and the post-hook deletes template/. All other contents, including Rust braces and dependency versions, remain unchanged.

The source tree stays valid Rust/TOML with no duplicate templated manifests. Generation needs cargo-generate and sed on Linux, not Python. The project-name value is substituted by Liquid, never interpolated into a shell command. Build/check/dev scripts also need no Python. Never approve unknown template hooks without reviewing them. The fixed hook name must be updated if the original template package is renamed.
