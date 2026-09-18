#!/usr/bin/env python3
"""Validate local links and fragments in the rendered documentation site."""

from __future__ import annotations

import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "target/docs-site"


class PageParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.resources: list[str] = []
        self.images_without_alt = 0

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        values = dict(attrs)
        element_id = values.get("id")
        if element_id:
            self.ids.add(element_id)
        if tag == "a" and values.get("href"):
            self.links.append(values["href"])
        if tag == "link" and values.get("href"):
            self.resources.append(values["href"])
        if tag in {"script", "img", "source"} and values.get("src"):
            self.resources.append(values["src"])
        if tag == "img" and "alt" not in values:
            self.images_without_alt += 1


def rendered_target(page: Path, link_path: str) -> Path:
    if link_path.startswith("/"):
        target = SITE / unquote(link_path.lstrip("/"))
    else:
        target = page.parent / unquote(link_path)
    if link_path.endswith("/") or target.is_dir():
        target /= "index.html"
    return target.resolve()


def main() -> int:
    if not SITE.is_dir():
        print("rendered documentation site is missing; run mkdocs build first", file=sys.stderr)
        return 1

    pages: dict[Path, PageParser] = {}
    for path in SITE.rglob("*.html"):
        parser = PageParser()
        parser.feed(path.read_text(encoding="utf-8"))
        pages[path.resolve()] = parser

    errors: list[str] = []
    for page, parser in pages.items():
        if parser.images_without_alt:
            errors.append(
                f"{page.relative_to(SITE)}: "
                f"{parser.images_without_alt} image(s) have no alt attribute"
            )
        for href in parser.links:
            parsed = urlsplit(href)
            if parsed.scheme or parsed.netloc:
                continue
            target = rendered_target(page, parsed.path) if parsed.path else page
            try:
                target.relative_to(SITE.resolve())
            except ValueError:
                errors.append(f"{page.relative_to(SITE)}: link escapes site: {href}")
                continue
            if not target.exists():
                errors.append(f"{page.relative_to(SITE)}: missing target: {href}")
                continue
            if parsed.fragment and target.suffix == ".html":
                target_parser = pages.get(target)
                fragment = unquote(parsed.fragment)
                if target_parser is None or fragment not in target_parser.ids:
                    errors.append(
                        f"{page.relative_to(SITE)}: missing fragment "
                        f"{fragment!r} in {target.relative_to(SITE)}"
                    )
        for source in parser.resources:
            parsed = urlsplit(source)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            target = rendered_target(page, parsed.path)
            try:
                target.relative_to(SITE.resolve())
            except ValueError:
                errors.append(f"{page.relative_to(SITE)}: asset escapes site: {source}")
                continue
            if not target.exists():
                errors.append(f"{page.relative_to(SITE)}: missing asset: {source}")

    if errors:
        print("rendered documentation link check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "rendered documentation link and asset check passed: "
        f"{len(pages)} HTML pages"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
