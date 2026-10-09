# GPUI project instructions

Read [llm-docs/OVERVIEW.md](llm-docs/OVERVIEW.md), [architecture](llm-docs/ARCHITECTURE.md), [extension recipes](llm-docs/EXTENDING.md), and [verification](llm-docs/VERIFY.md) before changes.

- README files and human documentation are read-only to agents. Generated documentation belongs under root `llm-docs/`, using OVERVIEW.md, not README.md. Agent instructions belong in AGENTS.md.
- Views own rendering/local interaction. Feature owners own longer-lived state and workflow decisions. Connectors own external transport/decoding; storage owns file/provider mechanics. Startup constructs dependencies; the shell retains owners and delivers opaque updates.
- Do not introduce an all-purpose app struct, event bus, universal connector trait, global product-dialog registry or catch-all utils module.
- All in-crate suites live in the owning module's `tests/<subject>.rs`, declared by that production module with cfg(test)/path. No inline suites, sibling *_tests.rs or production visibility widening to relocate tests.
- Single-suite helpers stay local; owner-shared helpers use tests/support; cross-feature helpers use test-only src/test_support. Root tests/ is for public-library integration crates only.
- Run `./scripts/check.sh` from this repository (works from any directory). Automated tests use fake requests/time, temporary files and disposable loopback listeners only. Never duplicate production workflows under cfg(test).
- Native GUI launch/automation and real keyring access require separate explicit consent. Never unlock/bypass a locked desktop. Use an unlocked isolated test session; no real accounts/secrets in logs, tests or screenshots.
- Human development uses `./dev.sh` with worktree-local .env.dev-config. Do not copy profiles from another worktree. Configuration isolation does not isolate a credential provider if one is added.
- This starter is unlicensed pending the owner's decision; do not publish or invent a license grant.
