# Customize LazyGUI

Paths are relative to the repository root. The catalog/HTTP demonstration has already been replaced by connected repository, working-tree, history and commit owners. Do not follow historical catalog-removal instructions as current runtime setup.

## Naming and presentation

The current package is `lazygui`; keep manifest/lock root entries consistent and dependencies locked. Library imports in `src/main.rs` and binary views use `lazygui` explicitly: renaming the package requires updating those imports (or declaring an intentional library name), not merely replacing Cargo text.

Native title and CLI usage are currently literal `LazyGUI`/`lazygui`; GUI profile namespace uses `env!("CARGO_PKG_NAME")`. Change these intentionally. `src/theme.rs` installs Kit dark theme; repository view also contains explicit product colors/layout. Assets are Kit's bundled assets, not a copied Hamlet profile. Add assets only with recorded ownership/licensing.

Startup reads existing/default GUI preferences, using window size, and trust; it does not currently save resize/recent paths or expose trust approval. Do not advertise automatic preference persistence without connecting ordered writes.

## Replace or extend the domain

1. Design focused feature owners and typed connectors. Follow [EXTENDING.md](EXTENDING.md), not a global AppState/event bus/universal executor.
2. Update `src/lib.rs` declarations, startup dependency construction and `src/views/app_shell/mod.rs` retention/delivery as one connected slice. Views never spawn Git/process operations.
3. Preserve one opaque result consumer per owner, identity/generation rejection and nonblocking retained shutdown. Shared gates cover actual index/worktree/common-dir resources.
4. Update private repository/control composition and scoped key/help definitions. Keep native editing/popup suppression and explicit unavailable labels.
5. Keep shared LazyGit YAML read-only and GUI profile mechanics separate. Remove a storage dependency only after its lifecycle/flush callers are deliberately replaced.
6. Remove unused owner/connector/views/fixtures and dependencies together; inspect all callers before pruning Cargo.lock.
7. Run `./scripts/check.sh`; update these agent docs and exact acceptance evidence. Human README files remain read-only.

Do not restore `CATALOG_URL`, demo HTTP or submitted-query persistence as Git configuration/workflow policy. No fixed sample request deadline or automatic write retry.

## Historical scaffolding versus current product

Original cargo-generate/plain-copy smoke evidence remains in [VERIFICATION-RESULTS.md](VERIFICATION-RESULTS.md). The old generation hook assumed the root package name `gpui-template`; current product/package/library/title changes have **not** been revalidated as a distributable generic template. Do not claim that old rename smoke tests verify today's LazyGUI. Inspect and update/retest scaffolding separately if the owner requests it.

Linux native development uses `./dev.sh` with worktree-local `.env.dev-config`; never copy another profile. Native launch/automation needs separate consent and an unlocked isolated session. Configuration isolation is not credential-provider isolation.

This project remains unlicensed pending the owner's decision. No publication or license grant is implied.
