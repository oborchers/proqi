#!/usr/bin/env python3
"""Validate local links and fragments in the rendered documentation site."""

from __future__ import annotations

import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "target/docs-site"
MKDOCS_CONFIG = ROOT / "mkdocs.yml"


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
        link_rel = set((values.get("rel") or "").split())
        asset_rel = {
            "icon",
            "manifest",
            "modulepreload",
            "preconnect",
            "preload",
            "stylesheet",
        }
        if tag == "link" and values.get("href") and link_rel & asset_rel:
            self.resources.append(values["href"])
        if tag in {"script", "img", "source"} and values.get("src"):
            self.resources.append(values["src"])
        if tag == "img" and "alt" not in values:
            self.images_without_alt += 1


def configured_site_prefix(config: Path) -> str:
    for line in config.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition(":")
        if key.strip() == "site_url" and separator:
            return urlsplit(value.strip().strip('"\'')).path.rstrip("/")
    return ""


def rendered_target(
    site: Path, page: Path, link_path: str, site_prefix: str = ""
) -> Path:
    if link_path.startswith("/"):
        relative = link_path
        if site_prefix and (
            relative == site_prefix or relative.startswith(f"{site_prefix}/")
        ):
            relative = relative[len(site_prefix) :] or "/"
        target = site / unquote(relative.lstrip("/"))
    else:
        target = page.parent / unquote(link_path)
    if link_path.endswith("/") or target.is_dir():
        target /= "index.html"
    return target.resolve()


def outside_deployment_prefix(link_path: str, site_prefix: str) -> bool:
    return bool(
        site_prefix
        and link_path.startswith("/")
        and link_path != site_prefix
        and not link_path.startswith(f"{site_prefix}/")
    )


def validation_errors(site: Path, site_prefix: str = "") -> tuple[list[str], int]:
    site = site.resolve()
    pages: dict[Path, PageParser] = {}
    for path in site.rglob("*.html"):
        parser = PageParser()
        parser.feed(path.read_text(encoding="utf-8"))
        pages[path.resolve()] = parser

    errors: list[str] = []
    if not pages:
        errors.append("rendered site contains no HTML pages")
    for page, parser in pages.items():
        if parser.images_without_alt:
            errors.append(
                f"{page.relative_to(site)}: "
                f"{parser.images_without_alt} image(s) have no alt attribute"
            )
        for href in parser.links:
            parsed = urlsplit(href)
            if parsed.scheme or parsed.netloc:
                continue
            if outside_deployment_prefix(parsed.path, site_prefix):
                errors.append(
                    f"{page.relative_to(site)}: link is outside deployment prefix: "
                    f"{href}"
                )
                continue
            target = (
                rendered_target(site, page, parsed.path, site_prefix)
                if parsed.path
                else page
            )
            try:
                target.relative_to(site.resolve())
            except ValueError:
                errors.append(f"{page.relative_to(site)}: link escapes site: {href}")
                continue
            if not target.exists():
                errors.append(f"{page.relative_to(site)}: missing target: {href}")
                continue
            if parsed.fragment and target.suffix == ".html":
                target_parser = pages.get(target)
                fragment = unquote(parsed.fragment)
                if target_parser is None or fragment not in target_parser.ids:
                    errors.append(
                        f"{page.relative_to(site)}: missing fragment "
                        f"{fragment!r} in {target.relative_to(site)}"
                    )
        for source in parser.resources:
            parsed = urlsplit(source)
            if parsed.scheme or parsed.netloc:
                errors.append(
                    f"{page.relative_to(site)}: external asset: {source}"
                )
                continue
            if not parsed.path:
                continue
            if outside_deployment_prefix(parsed.path, site_prefix):
                errors.append(
                    f"{page.relative_to(site)}: asset is outside deployment prefix: "
                    f"{source}"
                )
                continue
            target = rendered_target(site, page, parsed.path, site_prefix)
            try:
                target.relative_to(site.resolve())
            except ValueError:
                errors.append(f"{page.relative_to(site)}: asset escapes site: {source}")
                continue
            if not target.exists():
                errors.append(f"{page.relative_to(site)}: missing asset: {source}")

    return errors, len(pages)


def main() -> int:
    if not SITE.is_dir():
        print("rendered documentation site is missing; run mkdocs build first", file=sys.stderr)
        return 1

    errors, page_count = validation_errors(
        SITE, configured_site_prefix(MKDOCS_CONFIG)
    )

    if errors:
        print("rendered documentation link check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "rendered documentation link and asset check passed: "
        f"{page_count} HTML pages"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
