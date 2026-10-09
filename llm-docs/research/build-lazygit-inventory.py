#!/usr/bin/env python3
"""Extract a research index, not a complete executable compatibility specification.

Usage: python3 llm-docs/research/build-lazygit-inventory.py SOURCE DEFAULTS OUTPUT
SOURCE must be a checkout of the exact pinned commit. DEFAULTS is stdout from
`lazygit --config` on Linux. Uses Python's standard library only. Never runs
upstream code or commands from configuration. Output carries upstream MIT notice.
"""
import csv
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

COMMIT = "c5f7158154602d23b0750d4304c73e2aead8df5b"
BASE = f"https://github.com/jesseduffield/lazygit/blob/{COMMIT}/"


def emit_csv(path, rows, fields):
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    source, defaults, out = map(Path, sys.argv[1:])
    actual = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != COMMIT:
        raise SystemExit(f"Expected {COMMIT}, found {actual}")
    out.mkdir(parents=True, exist_ok=True)
    input_paths = {"docs/keybindings/Keybindings_en.md", "schema/config.json", "LICENSE"}
    bindings, context = [], ""
    for number, line in enumerate(
        (source / "docs/keybindings/Keybindings_en.md").read_text().splitlines(), 1
    ):
        if line.startswith("## "):
            context = line[3:]
        if not line.startswith("| `` "):
            continue
        parts = re.split(r"(?<!\\)\|", line)
        if len(parts) != 5:
            raise ValueError(f"Unexpected table row at {number}: {line}")
        keys, action, info = (part.strip().replace(r"\|", "|") for part in parts[1:4])
        bindings.append(dict(
            id=f"LG-B{len(bindings) + 1:03}", context=context,
            documented_keys=keys.removeprefix("`` ").removesuffix(" ``"),
            action=action, upstream_info=info, implementation="not_implemented",
            semantic_review="pending", acceptance="pending",
            source=BASE + f"docs/keybindings/Keybindings_en.md#L{number}",
        ))
    emit_csv(out / "bindings.csv", bindings, list(bindings[0]))

    schema = json.loads((source / "schema/config.json").read_text())
    definitions = schema["$defs"]
    config_rows = []

    def resolve(node):
        while "$ref" in node:
            node = definitions[node["$ref"].rsplit("/", 1)[1]]
        return node

    def walk(node, path, ancestors=()):
        if "$ref" in node:
            reference = node["$ref"]
            if reference in ancestors:
                config_rows.append(dict(path=path, type="recursive", description="", mapping="needs_review"))
                return
            ancestors = (*ancestors, reference)
        node = resolve(node)
        if "properties" in node:
            for name, child in node["properties"].items():
                walk(child, f"{path}.{name}" if path else name, ancestors)
        elif node.get("type") == "array" and "items" in node:
            walk(node["items"], path + "[]", ancestors)
        else:
            config_rows.append(dict(
                path=path, type=json.dumps({k: node[k] for k in ("type", "enum", "oneOf", "additionalProperties") if k in node}, sort_keys=True),
                description=node.get("description", ""), mapping="needs_review",
            ))

    walk({"$ref": schema["$ref"]}, "")
    emit_csv(out / "config-fields.csv", config_rows, ["path", "type", "description", "mapping"])

    functions = []
    roots = ("pkg/gui/controllers", "pkg/gui/services/custom_commands", "pkg/commands/git_commands", "pkg/commands/patch")
    for root in roots:
        for file in sorted((source / root).rglob("*.go")):
            if file.name.endswith("_test.go"):
                continue
            relative = file.relative_to(source).as_posix()
            input_paths.add(relative)
            for number, line in enumerate(file.read_text().splitlines(), 1):
                if line.startswith("func "):
                    functions.append(dict(
                        file=relative, line=number, signature=line,
                        review="pending", source=BASE + f"{relative}#L{number}",
                    ))
    emit_csv(out / "workflow-functions.csv", functions, ["file", "line", "signature", "review", "source"])
    tests = []
    for file in sorted((source / "pkg/integration/tests").rglob("*.go")):
        relative = file.relative_to(source).as_posix()
        tests.append(dict(
            category=file.parent.name, file=relative,
            fixture_review="pending", ported_acceptance="pending", source=BASE + relative,
        ))
    emit_csv(out / "upstream-test-files.csv", tests, ["category", "file", "fixture_review", "ported_acceptance", "source"])
    (out / "defaults-linux.yml").write_bytes(defaults.read_bytes())
    (out / "upstream-LICENSE.txt").write_bytes((source / "LICENSE").read_bytes())
    hashes = {path: hashlib.sha256((source / path).read_bytes()).hexdigest() for path in sorted(input_paths)}
    manifest = dict(
        upstream="https://github.com/jesseduffield/lazygit", tag="v0.66.0", commit=COMMIT,
        default_export_command="lazygit --config", default_export_platform="linux",
        default_export_sha256=hashlib.sha256(defaults.read_bytes()).hexdigest(),
        counts=dict(documented_binding_rows=len(bindings), documented_binding_contexts=len({r['context'] for r in bindings}), schema_field_rows=len(config_rows), production_function_rows=len(functions), integration_go_files=len(tests)),
        limitations=[
            "Bindings come from the generated English cheat sheet, not runtime dispatch.",
            "Navigation keys without descriptions, editor internals, menu mnemonics, custom bindings and conditional guards need source review.",
            "Schema rows are field paths, not promises that every setting is supported; recursive command menus are indexed once.",
            "Function signatures are a source index, not unique user actions or a complete call graph.",
            "Integration Go files include helpers and demos, not just runnable tests.",
            "No upstream tests, GUI sessions, hooks, credentials or config-defined commands were executed.",
        ],
        input_sha256=hashes,
    )
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest["counts"], indent=2))


if __name__ == "__main__":
    main()
