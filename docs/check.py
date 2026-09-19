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


def named_body(source: str, kind: str, name: str) -> str:
    marker = f"{kind} {name} "
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


def enum_entries(source: str, name: str) -> list[tuple[str, str, str | None, str]]:
    body = named_body(source, "enum", name)
    raw_entries: list[tuple[int, str, str, str | None, bool]] = []
    depth = 0
    command_attribute = ""
    lines = body.splitlines()
    for index, line in enumerate(lines):
        if depth == 0 and line.strip().startswith("#[command("):
            command_attribute = line
            continue
        if depth == 0:
            match = re.match(
                r"\s*([A-Z][A-Za-z0-9_]*)\s*"
                r"(?:\(\s*([A-Z][A-Za-z0-9_]*)\s*\)|\{|,)",
                line,
            )
            if match:
                variant, payload = match.groups()
                hidden = bool(
                    re.search(r"\bhide\s*=\s*true\b", command_attribute)
                )
                explicit = re.search(r'\bname\s*=\s*"([^"]+)"', command_attribute)
                command = explicit.group(1) if explicit else kebab_case(variant)
                raw_entries.append((index, variant, command, payload, hidden))
                command_attribute = ""
        depth += line.count("{") - line.count("}")

    entries: list[tuple[str, str, str | None, str]] = []
    for offset, entry in enumerate(raw_entries):
        start, variant, command, payload, hidden = entry
        end = raw_entries[offset + 1][0] if offset + 1 < len(raw_entries) else len(lines)
        if not hidden:
            entries.append((variant, command, payload, "\n".join(lines[start:end])))
    return entries


def enum_commands(source: str, name: str) -> list[tuple[str, str]]:
    return [(variant, command) for variant, command, _, _ in enum_entries(source, name)]


def kebab_case(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def subcommand_field(source: str, arguments: str) -> tuple[str, bool]:
    body = named_body(source, "struct", arguments)
    match = re.search(
        r"#\[command\(subcommand\)\]\s*"
        r"(?:pub\(super\)\s+)?command\s*:\s*(Option<)?"
        r"([A-Z][A-Za-z0-9_]*)(?:>)?",
        body,
    )
    if match is None:
        raise ValueError(f"{arguments} does not own a subcommand field")
    return match.group(2), match.group(1) is not None


def flags_in_block(source: str) -> set[str]:
    flags: set[str] = set()
    fields = re.finditer(
        r"#\[arg\((.*?)\)\]\s*(?:pub\(super\)\s+)?"
        r"([a-z][A-Za-z0-9_]*)\s*:",
        source,
        re.DOTALL,
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


def public_cli_contract(source: str) -> tuple[set[str], set[str], dict[str, set[str]]]:
    surfaces: set[str] = set()
    callable_parents: set[str] = set()
    surface_flags: dict[str, set[str]] = {}
    for _, command, arguments, block in enum_entries(source, "Command"):
        if arguments is None:
            surfaces.add(command)
            surface_flags[command] = flags_in_block(block)
            continue

        nested_enum, optional = subcommand_field(source, arguments)
        if optional:
            surfaces.add(command)
            callable_parents.add(command)
            surface_flags[command] = set()
        for _, nested_command, nested_arguments, nested_block in enum_entries(
            source, nested_enum
        ):
            surface = f"{command} {nested_command}"
            surfaces.add(surface)
            flags = flags_in_block(nested_block)
            if nested_arguments is not None:
                flags.update(
                    flags_in_block(named_body(source, "struct", nested_arguments))
                )
            surface_flags[surface] = flags
    return surfaces, callable_parents, surface_flags


def public_cli_surfaces(source: str) -> set[str]:
    return public_cli_contract(source)[0]


def public_cli_flags(source: str) -> set[str]:
    return {"-h", "--help", "-V", "--version"} | flags_in_block(
        named_body(source, "struct", "Cli")
    )


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

    cli_surfaces, callable_parents, surface_flags = public_cli_contract(cli_source)
    for surface in sorted(cli_surfaces):
        suffix = r"\s*$" if surface in callable_parents else r"(?:\s|$)"
        invocation = re.compile(
            rf"^proqi(?:\s+--json)?\s+{re.escape(surface)}{suffix}", re.MULTILINE
        )
        if invocation.search(cli_doc) is None:
            errors.append(f"public CLI surface {surface!r} is missing from cli.md")
            continue
        synopsis = re.compile(
            rf"^proqi(?:\s+--json)?\s+{re.escape(surface)}(?:\s.*)?$",
            re.MULTILINE,
        )
        documented = "\n".join(synopsis.findall(cli_doc))
        for flag in sorted(surface_flags[surface]):
            if not has_cli_token(documented, flag):
                errors.append(
                    f"public CLI flag {flag!r} for {surface!r} is missing "
                    "from its cli.md synopsis"
                )

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
