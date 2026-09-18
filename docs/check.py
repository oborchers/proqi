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


def enum_commands(source: str, name: str) -> list[tuple[str, str]]:
    body = enum_body(source, name)
    commands: list[tuple[str, str]] = []
    depth = 0
    command_attribute = ""
    for line in body.splitlines():
        if depth == 0 and line.strip().startswith("#[command("):
            command_attribute = line
            continue
        if depth == 0:
            match = re.match(r"\s*([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)", line)
            if match:
                variant = match.group(1)
                hidden = re.search(r"\bhide\s*=\s*true\b", command_attribute)
                explicit = re.search(r'\bname\s*=\s*"([^"]+)"', command_attribute)
                if hidden is None:
                    commands.append(
                        (variant, explicit.group(1) if explicit else kebab_case(variant))
                    )
                command_attribute = ""
        depth += line.count("{") - line.count("}")
    return commands


def kebab_case(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def public_cli_surfaces(source: str) -> set[str]:
    root = {
        command
        for variant, command in enum_commands(source, "Command")
        if variant not in {"Update", "Diagnostics", "Sessions", "Thoughts"}
    }
    nested = {
        "diagnostics": "DiagnosticsCommand",
        "update": "UpdateCommand",
        "sessions": "SessionCommand",
        "thoughts": "ThoughtCommand",
    }
    for prefix, enum in nested.items():
        root.update(f"{prefix} {command}" for _, command in enum_commands(source, enum))
        if re.search(rf"command:\s*Option<{re.escape(enum)}>", source):
            root.add(prefix)
    return root


def public_cli_flags(source: str) -> set[str]:
    flags = {"-h", "--help", "-V", "--version"}
    fields = re.finditer(
        r"^\s*#\[arg\((.*)\)\]\s*\n\s*(?:pub\(super\)\s+)?"
        r"([a-z][A-Za-z0-9_]*)\s*:",
        source,
        re.MULTILINE,
    )
    for field in fields:
        options, name = field.groups()
        if re.search(r"\bhide\s*=\s*true\b", options):
            continue
        explicit_long = re.search(r'\blong\s*=\s*"([^"]+)"', options)
        if explicit_long:
            flags.add(f"--{explicit_long.group(1)}")
        elif re.search(r"\blong\b", options):
            flags.add(f"--{name.replace('_', '-')}")
        explicit_short = re.search(r"\bshort\s*=\s*'([^']+)'", options)
        if explicit_short:
            flags.add(f"-{explicit_short.group(1)}")
        elif re.search(r"\bshort\b", options):
            flags.add(f"-{name[0]}")
    return flags


def has_cli_token(documentation: str, token: str) -> bool:
    return re.search(
        rf"(?<![A-Za-z0-9_-]){re.escape(token)}(?![A-Za-z0-9_-])", documentation
    ) is not None


def coverage_errors(
    action_source: str, commands_doc: str, cli_source: str, cli_doc: str
) -> tuple[list[str], int, int]:
    labels = registered_command_labels(action_source)
    errors: list[str] = []
    if len(set(labels)) != len(labels):
        errors.append("ShortcutActionId::COMMANDS contains duplicate labels")
    if f"{len(labels)} actions" not in commands_doc:
        errors.append(
            f"commands.md must identify the current {len(labels)}-action inventory"
        )
    for label in labels:
        count = commands_doc.count(f"**{label}**")
        if count != 1:
            errors.append(
                f"Commands label {label!r} must appear once in commands.md, found {count}"
            )

    cli_surfaces = public_cli_surfaces(cli_source)
    for surface in sorted(cli_surfaces):
        invocation = re.compile(
            rf"^proqi(?:\s+--json)?\s+{re.escape(surface)}(?:\s|$)", re.MULTILINE
        )
        if invocation.search(cli_doc) is None:
            errors.append(f"public CLI surface {surface!r} is missing from cli.md")

    for flag in sorted(public_cli_flags(cli_source)):
        if not has_cli_token(cli_doc, flag):
            errors.append(f"public CLI flag {flag!r} is missing from cli.md")

    startup_forms = {
        "plain startup": r"^proqi\s*$",
        "continue startup": r"^proqi (?:-c|--continue)\s*$",
        "resume startup": r"^proqi (?:-r|--resume)(?:\s|$)",
        "JSON mode": r"^proqi --json(?:\s|$)",
    }
    for label, pattern in startup_forms.items():
        if re.search(pattern, cli_doc, re.MULTILINE) is None:
            errors.append(f"public CLI {label} is missing from cli.md")

    return errors, len(labels), len(cli_surfaces)


def main() -> int:
    errors, action_count, cli_count = coverage_errors(
        ACTION_SOURCE.read_text(encoding="utf-8"),
        COMMANDS_DOC.read_text(encoding="utf-8"),
        CLI_SOURCE.read_text(encoding="utf-8"),
        CLI_DOC.read_text(encoding="utf-8"),
    )
    if errors:
        print("documentation coverage check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        f"documentation coverage check passed: {action_count} Commands actions and "
        f"{cli_count} CLI surfaces"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
