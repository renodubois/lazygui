# Extending this project

All source paths are relative to the project root. `<name>` denotes a future module, not an existing file. The runnable example uses a non-rendering `Entity<Catalog>`: views observe it with GPUI subscriptions; the shell alone consumes its opaque result receiver. Replace domain names as needed, keeping ownership intact.

### File-placement decision table

| I want to add… | Put it here | Wire it from… |
| --- | --- | --- |
| A standalone screen | `src/views/<name>.rs`, or `<name>/mod.rs` if it owns children | `views/mod.rs`, then shell's screen selection/composition |
| A screen-private panel/dialog | `src/views/<owner>/<name>.rs` | Owning view's `mod.rs` and constructor |
| Truly reused presentation | `src/views/shared/<purpose>.rs` | Its actual consuming views; create only on real reuse |
| Longer-lived product behavior | `src/<feature>/{mod,state}.rs` | Declare in `main.rs`; host above replaceable views |
| External-service connector | `src/connectors/<provider>/` | `connectors/mod.rs`; construct dependencies in startup/host |
| Another operation on an existing provider | Its connector operation file/interface | The feature that requests it; not directly from a view |
| External wire DTOs | Private connector file such as `wire.rs` | Connector decoder; convert to public data where useful |
| Data shared across features | Small focused data module if genuinely shared | Only modules needing that vocabulary; avoid a catch-all model |
| File/config mechanics | `src/storage/` | Dependency construction and the owning workflow |
| Authentication/restore policy | Optional `src/session/` | Stable host and requesting features, not login controls |
| Credential-provider mechanics | Optional `src/storage/credentials.rs` | Ordered storage interface, never views |
| Theme role/token changes | `src/theme.rs` | Startup already installs theme |
| Feature/view/connector tests | Owning module's `tests/<subject>.rs` | Declare from its production owner under `cfg(test)` |
| Shared test fixture | Owner's `tests/support/`, or `src/test_support/` for cross-feature use | Test-only module declarations |

M0 adds a separately compiled capability library in `src/lib.rs`; its prototypes and owner-local suites are mapped in [ARCHITECTURE.md](ARCHITECTURE.md) and [M0-RESULTS.md](M0-RESULTS.md). When connecting M1, use those focused interfaces rather than passing the capability library as a product-wide dependency bag. Preserve the catalog until its owner/connector/views are replaced as one connected slice. The Go template oracle is not an approved production dependency.

A feature can legitimately touch behavior, connectors and views. Prefer predictable ownership over putting all end-to-end code into one folder.

### Recipe: add a new view

1. Decide whether it is a screen, owner-private child or genuinely shared presentation. Use `views/settings.rs` for a small screen; promote it to `views/settings/mod.rs` when it acquires children. Never retain both module forms simultaneously.
2. Declare `mod settings;` in `views/mod.rs`. Keep the module private unless an actual caller outside views needs it; prefer `pub(super)` for view-local constructors.
3. Implement rendering and local interaction state. Use a GPUI entity when stateful; a stateless rendering helper need not become an entity. Use Kit controls and stable semantic IDs/accessibility labels.
4. Accept only the feature handles/data required by the screen. Do not pass the root view, a mutable global app struct, raw HTTP client or credential string. If new behavior must outlive this screen, create `src/<feature>/` and let the stable host retain it.
5. Construct child views inside their parent screen; register screen selection/navigation in `app_shell.rs`. The shell chooses screens without knowing child layout internals.
6. Keep focus, unsubmitted form edits and scroll/selection local. Keep authoritative shared records, persisted preferences and long-lived drafts in their owning feature. Do not maintain a second authoritative copy in controls.
7. Retain subscriptions/tasks for the intended lifetime. Test dropping/recreating the view while a request is pending: work/state survive only where the owner policy requires, and no duplicate result consumers are created.
8. Put local suites in `views/settings/tests/` (or `views/tests/settings.rs` for a single-file view), declared by that owner. Put cross-screen journeys in `views/tests/`. Exercise production constructors and semantic controls, not private fields or a fixed render tree.
9. Update the architecture placement map and run automated checks. Native keyboard/IME/accessibility acceptance is a separate consented step.

### Recipe: add an external connector (for example, GitHub)

1. Define the product capability first: e.g. “list repository issues,” including inputs, typed data, failures and timeout semantics. Do not begin with a generic `Connector::request` or shared trait for unrelated providers.
2. Create `connectors/github/mod.rs`, `client.rs`, `issues.rs`, `types.rs`, `error.rs`, and `tests/{binding,http}.rs`; add `mod github;` to `connectors/mod.rs`. These are recipe paths, not empty files to pre-create in the base.
3. Keep URL validation, paths, headers, serialization, status/error decoding, redirects, TLS and request deadlines inside the connector. Expose typed operations and a context-bound client, not reqwest types. Review provider-specific base-path/proxy/auth requirements; do not blindly reuse Hamlet's root-origin validator.
4. Bind endpoint/account credentials immutably when creating the client. Replacing an account creates a new context. Do not look up a mutable global token on each request; do not leak secrets via `Debug`, errors or logs.
5. Construct the real adapter in `main.rs` or the stable host. Inject it into the feature that owns issue-loading behavior. The view submits an intention to that feature and reads feature state; it never executes the provider operation itself.
6. Add an internal substitution seam with real and controlled test adapters where request outcomes vary. Scope the interface to the capability the caller uses. A Notion connector can expose different operations: sharing external I/O does not establish interchangeable semantics.
7. Keep in-flight ownership, generations, retries and user-facing uncertainty in the feature. Connector errors are typed, not preformatted product UI. Do not replay writes automatically after timeout unless an explicit idempotency policy supports it.
8. For tokens, implement the authentication/credential recipe first: nonsensitive metadata in files, secrets in an appropriate provider, background blocking work, ordered save/delete, observable ambiguous outcomes, and context isolation. Never persist production tokens in a sample fixture or enable live-account tests by default.
9. Test both the connector seam and actual loopback wire behavior: malformed responses, binding/auth headers, redirects, timeouts, cancellation and redacted errors. Add feature tests for stale outcomes/account changes and view tests for loading/errors. Automated tests use disposable fake credentials and no external provider account.
10. Document required configuration and platform constraints in `OVERVIEW.md`/`EXTENDING.md`, then run checks. Live-provider experiments need an explicit isolated setup; real keyring access is not an ordinary build/test operation.

### Recipe: add a feature, storage or dialog

- **Feature:** add `src/<feature>/mod.rs` for coordination and `state.rs` for cohesive pure transitions; declare it in `main.rs`, inject dependencies, and retain its handle in the appropriate host. Document whether it is window-, document-, session- or process-scoped. Tests cross its same intention/read interface as callers. Do not duplicate workflows behind `cfg(test)`.
- **Storage:** add file/provider mechanics under `storage/`; configuration data is not a general store for all product state. Define format/version, failure/atomicity behavior and migration policy. A feature decides persistence policy. If operations race (especially credential save/delete), serialize the protocol rather than create independent writers. Use temporary paths/fake providers in tests.
- **Dialog:** put dialog content/state under its owning view, promoting the view to a directory if needed. Use Kit's window dialog host for overlay/focus/dismissal infrastructure; keep validation and operation policy with the feature. Closing a dialog is not proof that a submitted write was canceled.
- **Tests:** preserve Rust ownership/private access with `#[cfg(test)] #[path = "tests/<subject>.rs"] mod tests;`. No inline suites or sibling `*_tests.rs`. Test-only hooks are narrow and do not widen normal visibility. Root `tests/` is only for a library's public-interface integration tests; the base is a binary crate.


Every feature mutation through an entity must call `cx.notify()` after changing state. Entity observation is broadcast invalidation; the completion receiver is single-consumer. A new screen uses the existing feature entity rather than creating another owner. Review [ARCHITECTURE.md](ARCHITECTURE.md) and [VERIFY.md](VERIFY.md) before extending.
