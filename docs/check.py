#!/usr/bin/env python3
"""Validate that user documentation covers the shipped discovery surfaces."""

from __future__ import annotations

import ast
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ACTION_SOURCE = ROOT / "src/ui/shortcut_registry/model.rs"
COMMANDS_DOC = ROOT / "docs/reference/commands.md"
CLI_DOC = ROOT / "docs/reference/cli.md"
CLI_SOURCE = ROOT / "src/cli/args.rs"


def commands_block(source: str) -> str:
    marker = "pub(crate) const COMMANDS:"
    start = source.find(marker)
    if start < 0:
        raise ValueError("ShortcutActionId::COMMANDS was not found")
    start = source.find("= [", start)
    if start < 0:
        raise ValueError("ShortcutActionId::COMMANDS initializer was not found")
    start += 2
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "[":
            depth += 1
        elif source[index] == "]":
            depth -= 1
            if depth == 0:
                return source[start + 1 : index]
    raise ValueError("ShortcutActionId::COMMANDS did not have a closing bracket")


def registered_command_labels(source: str) -> list[str]:
    block = commands_block(source)
    literals = re.findall(r'"(?:[^"\\]|\\.)*"', block)
    return [ast.literal_eval(literal) for literal in literals]


def enum_body(source: str, name: str) -> str:
    marker = f"enum {name} "
    start = source.find(marker)
    if start < 0:
        raise ValueError(f"{name} was not found")
    start = source.find("{", start)
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start + 1 : index]
    raise ValueError(f"{name} did not have a closing brace")


def enum_variants(source: str, name: str) -> list[str]:
    body = enum_body(source, name)
    variants: list[str] = []
    depth = 0
    for line in body.splitlines():
        if depth == 0:
            match = re.match(r"\s*([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)", line)
            if match:
                variants.append(match.group(1))
        depth += line.count("{") - line.count("}")
    return variants


def kebab_case(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def public_cli_surfaces(source: str) -> set[str]:
    root = {
        kebab_case(variant)
        for variant in enum_variants(source, "Command")
        if variant not in {"AttachmentCheckWorker", "Update", "Diagnostics", "Sessions", "Thoughts"}
    }
    nested = {
        "diagnostics": "DiagnosticsCommand",
        "update": "UpdateCommand",
        "sessions": "SessionCommand",
        "thoughts": "ThoughtCommand",
    }
    for prefix, enum in nested.items():
        root.update(
            f"{prefix} {kebab_case(variant)}" for variant in enum_variants(source, enum)
        )
    return root


def main() -> int:
    source = ACTION_SOURCE.read_text(encoding="utf-8")
    commands_doc = COMMANDS_DOC.read_text(encoding="utf-8")
    cli_doc = CLI_DOC.read_text(encoding="utf-8")
    labels = registered_command_labels(source)

    errors: list[str] = []
    if len(labels) != 59:
        errors.append(f"expected 59 registered Commands labels, found {len(labels)}")
    for label in labels:
        count = commands_doc.count(f"**{label}**")
        if count != 1:
            errors.append(
                f"Commands label {label!r} must appear once in commands.md, found {count}"
            )

    cli_surfaces = public_cli_surfaces(CLI_SOURCE.read_text(encoding="utf-8"))
    for surface in sorted(cli_surfaces):
        if surface not in cli_doc:
            errors.append(f"public CLI surface {surface!r} is missing from cli.md")

    if errors:
        print("documentation coverage check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        f"documentation coverage check passed: {len(labels)} Commands actions and "
        f"{len(cli_surfaces)} CLI surfaces"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
