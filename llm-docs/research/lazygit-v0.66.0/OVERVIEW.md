# Pinned LazyGit research artifacts

Baseline: LazyGit v0.66.0, commit `c5f7158154602d23b0750d4304c73e2aead8df5b`, Linux default bindings. Collected 2026-10-08.

Read the [investigation](../../LAZYGIT-INVESTIGATION.md) and [proposed plan](../../LAZYGUI-PLAN.md) before treating any index as a specification.

## Files

- `bindings.csv`: 301 generated English cheatsheet action rows across 22 sections, with original context/key/action/information, source links and pending implementation/review/acceptance fields. Key labels are upstream documentation strings, **not parsed normalized key definitions**. Section names aren't necessarily unique runtime context identifiers.
- `config-fields.csv`: 362 schema paths (170 keybinding, 107 GUI, 37 Git, and 48 other rows). Includes array element paths and recursive command-menu boundary. Dynamic maps aren't expanded into invented concrete keys. Type/description come from schema; native mapping is pending for every row.
- `workflow-functions.csv`: 1,975 production function signatures from Git command/patch code, GUI controllers/helpers and custom-command services. This is a navigation index, **not 1,975 features**, a call graph or a record that all bodies were reviewed. GUI setup/legacy bindings/config code are additional evidence outside these roots.
- `upstream-test-files.csv`: 677 integration Go file paths including fixture helpers and demos. Nothing here implies 677 runnable or ported tests. A subset is reviewed in the investigation; the machine index intentionally records full fixture/ported acceptance review as pending.
- `defaults-linux.yml`: unmodified stdout from the installed `lazygit --config`; full default values, not effective user config. OS settings with omitted empty values can be absent; use the schema/types too.
- `manifest.json`: tag/commit, counts, hashes of extraction source files, default-export hash and explicit extraction limitations.
- `upstream-LICENSE.txt`: unchanged upstream MIT notice applicable to the upstream-derived descriptions/defaults. Does not grant a license to this project.

## Reproduce

With network access, make a temporary shallow checkout outside the project:

```sh
research=$(mktemp -d)
git clone --depth 1 --branch v0.66.0 https://github.com/jesseduffield/lazygit.git "$research/lazygit"
# Requires the matching Linux binary; inspect --version before export.
lazygit --version
lazygit --config > "$research/defaults-linux.yml"
python3 llm-docs/research/build-lazygit-inventory.py \
  "$research/lazygit" "$research/defaults-linux.yml" \
  "$research/inventory"
```

The generator rejects a source checkout at any other commit. It uses Python's standard library only, reads source/schema/defaults and writes generated indexes. It does not build/run upstream source, launch a GUI, load user config into execution, run config commands, call Git on product repositories or touch credentials. Compare regenerated artifacts before replacing these files; the supplied binary defaults must also be pinned.

## Interpretation limits

The cheatsheet is generated and lossy. Hidden navigation/text-editor actions, menu mnemonics, controller attachment order, conditional guards/fallback dispatch, popup/search suppression and config-defined commands need explicit review. A copied key table cannot establish muscle-memory parity. Rows marked pending are research backlog, not functioning GUI actions.
