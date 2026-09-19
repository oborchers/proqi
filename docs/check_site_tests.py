from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import check_site


class RenderedSiteTests(unittest.TestCase):
    def test_complete_local_site_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "assets").mkdir()
            (site / "assets/style.css").write_text("body {}\n", encoding="utf-8")
            (site / "assets/image.png").write_bytes(b"fixture")
            (site / "index.html").write_text(
                '<link rel="stylesheet" href="assets/style.css">'
                '<a href="next.html#target">Next</a>'
                '<img src="assets/image.png" alt="Fixture">',
                encoding="utf-8",
            )
            (site / "next.html").write_text(
                '<h1 id="target">Target</h1>', encoding="utf-8"
            )

            errors, pages = check_site.validation_errors(site)

            self.assertEqual(errors, [])
            self.assertEqual(pages, 2)

    def test_missing_fragment_alt_and_external_asset_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "index.html").write_text(
                '<link rel="stylesheet" href="https://example.invalid/style.css">'
                '<a href="#missing">Missing</a><img src="missing.png">',
                encoding="utf-8",
            )

            errors, _ = check_site.validation_errors(site)

            self.assertTrue(any("external asset" in error for error in errors))
            self.assertTrue(any("missing fragment" in error for error in errors))
            self.assertTrue(any("no alt attribute" in error for error in errors))
            self.assertTrue(any("missing asset" in error for error in errors))

    def test_deployment_prefix_resolves_inside_static_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "assets").mkdir()
            (site / "assets/style.css").write_text("body {}\n", encoding="utf-8")
            (site / "index.html").write_text(
                '<link rel="stylesheet" href="/proqi/assets/style.css">'
                '<a href="/proqi/.">Home</a>',
                encoding="utf-8",
            )

            errors, pages = check_site.validation_errors(site, "/proqi")

            self.assertEqual(errors, [])
            self.assertEqual(pages, 1)

    def test_site_url_path_becomes_deployment_prefix(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "mkdocs.yml"
            config.write_text(
                "site_name: Example\nsite_url: https://example.invalid/proqi/\n",
                encoding="utf-8",
            )

            self.assertEqual(check_site.configured_site_prefix(config), "/proqi")

    def test_root_absolute_asset_outside_deployment_prefix_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "assets").mkdir()
            (site / "assets/style.css").write_text("body {}\n", encoding="utf-8")
            (site / "index.html").write_text(
                '<link rel="stylesheet" href="/assets/style.css">',
                encoding="utf-8",
            )

            errors, _ = check_site.validation_errors(site, "/proqi")

            self.assertTrue(any("outside deployment prefix" in error for error in errors))

    def test_empty_site_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            errors, pages = check_site.validation_errors(Path(directory))

            self.assertEqual(pages, 0)
            self.assertIn("rendered site contains no HTML pages", errors)


if __name__ == "__main__":
    unittest.main()
