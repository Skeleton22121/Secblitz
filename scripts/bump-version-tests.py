#!/usr/bin/env python3
"""Tests for bump-version.py and release-notes.py on a temporary copy of the repo files."""
import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
FILES = ["Cargo.toml", "Cargo.lock", "assets/secblitz.rc", "assets/secblitz.manifest",
         "CHANGELOG.md", "README.md", "website/index.html"]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), HERE / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bump = load("bump-version")
notes = load("release-notes")


def run(*args):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = bump.main(list(args))
    return code, out.getvalue(), err.getvalue()


class BumpTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        for rel in FILES:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / rel, self.root / rel)
        self.old = bump.current_version(self.root)
        # A fixed changelog, so the tests do not depend on the real release notes.
        (self.root / "CHANGELOG.md").write_text(
            "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- A test entry.\n\n"
            f"## [{self.old}] - Unreleased\n\n### Added\n- Work in progress.\n\n"
            "## [0.7.0] - 2026-10-05\n\n### Added\n- Seven.\n\n- More seven.\n\n"
            "## [0.6.1] - 2026-10-04\n\n### Changed\n- Six one.\n", encoding="utf-8")
        self.shown = bump.site_version((self.root / "README.md").read_text(encoding="utf-8"))

    def tearDown(self):
        self.tmp.cleanup()

    def snapshot(self):
        return {rel: (self.root / rel).read_bytes() for rel in FILES}

    def bump(self, version, *extra):
        return run(version, "--root", str(self.root), "--date", "2030-01-02", *extra)

    def test_bump_updates_every_place(self):
        before = self.snapshot()
        code, out, err = self.bump("9.8.7")
        self.assertEqual(code, 0, err)
        after = self.snapshot()
        cargo = after["Cargo.toml"].decode()
        self.assertIn('version = "9.8.7"', cargo)
        self.assertIn(f'name = "secblitz"\nversion = "9.8.7"', after["Cargo.lock"].decode())
        rc = after["assets/secblitz.rc"].decode()
        self.assertIn("FILEVERSION 9,8,7,0", rc)
        self.assertIn("PRODUCTVERSION 9,8,7,0", rc)
        self.assertIn('VALUE "FileVersion", "9.8.7"', rc)
        self.assertIn('VALUE "ProductVersion", "9.8.7"', rc)
        self.assertIn('assemblyIdentity version="9.8.7.0"', after["assets/secblitz.manifest"].decode())
        self.assertNotIn(self.old, rc)
        # Other lines are untouched.
        for rel in ("Cargo.toml", "Cargo.lock", "assets/secblitz.rc", "assets/secblitz.manifest"):
            a = before[rel].decode().splitlines()
            b = after[rel].decode().splitlines()
            self.assertEqual(len(a), len(b))
            self.assertLessEqual(sum(x != y for x, y in zip(a, b)), 4)
        # Site files only change with --site.
        self.assertEqual(before["README.md"], after["README.md"])
        self.assertEqual(before["website/index.html"], after["website/index.html"])

    def test_changelog_moves_unreleased_into_dated_section(self):
        self.assertEqual(self.bump("9.8.7")[0], 0)
        text = (self.root / "CHANGELOG.md").read_text(encoding="utf-8")
        self.assertIn("## [Unreleased]\n\n## [9.8.7] - 2030-01-02\n\n### Fixed\n- A test entry.\n", text)
        self.assertEqual(text.count("A test entry."), 1)
        self.assertEqual(notes.section(text, "9.8.7"), "### Fixed\n- A test entry.\n")

    def test_dry_run_leaves_tree_untouched(self):
        before = self.snapshot()
        code, out, err = self.bump("9.8.7", "--dry-run", "--site")
        self.assertEqual(code, 0, err)
        self.assertIn("Dry run", out)
        self.assertEqual(before, self.snapshot())
        self.assertEqual([p.name for p in self.root.rglob(".bump-*")], [])

    def test_refuses_same_older_and_malformed_versions(self):
        before = self.snapshot()
        major, minor, patch = (int(x) for x in self.old.split("."))
        # The current version is only accepted while its changelog section says Unreleased.
        path = self.root / "CHANGELOG.md"
        path.write_text(path.read_text().replace(f"## [{self.old}] - Unreleased", f"## [{self.old}] - 2026-10-07"))
        before = self.snapshot()
        for version in [f"{major}.{minor}.{patch}", "0.0.1", f"{major}.{minor}.{max(patch - 1, 0)}",
                        "1.2", "1.2.3.4", "v1.2.3", "01.2.3", "1.2.3-beta", ""]:
            code, out, err = self.bump(version)
            self.assertEqual(code, 1, version)
            self.assertTrue(err.startswith("error:"), version)
        self.assertEqual(before, self.snapshot())

    def test_numeric_comparison_not_text(self):
        # 0.10.0 is greater than 0.9.0 even though "0.10.0" < "0.9.0" as text.
        self.assertEqual(self.bump("0.9.0")[0], 0)
        path = self.root / "CHANGELOG.md"
        path.write_text(path.read_text().replace("## [Unreleased]\n", "## [Unreleased]\n\n- Another entry.\n", 1))
        self.assertEqual(self.bump("0.10.0", "--dry-run")[0], 0)
        self.assertEqual(self.bump("0.8.9", "--dry-run")[0], 1)

    def test_refuses_empty_unreleased_unless_allowed(self):
        path = self.root / "CHANGELOG.md"
        path.write_text(path.read_text().replace("- A test entry.\n", ""))
        before = self.snapshot()
        self.assertEqual(self.bump("9.8.7")[0], 1)
        self.assertEqual(before, self.snapshot())
        self.assertEqual(self.bump("9.8.7", "--allow-empty")[0], 0)

    def test_refuses_existing_version_section_and_missing_unreleased(self):
        path = self.root / "CHANGELOG.md"
        original = path.read_text()
        path.write_text(original.replace("## [Unreleased]", "## [Soon]", 1))
        self.assertEqual(self.bump("9.8.7")[0], 1)
        path.write_text(original.replace("## [0.7.0] - 2026-10-05", "## [9.8.7] - 2026-10-05"))
        self.assertEqual(self.bump("9.8.7")[0], 1)

    def test_unreleased_current_version_is_only_dated(self):
        path = self.root / "CHANGELOG.md"
        self.assertIn(f"## [{self.old}] - Unreleased", path.read_text())
        code, out, err = self.bump(self.old)
        self.assertEqual(code, 0, err)
        text = path.read_text()
        self.assertIn(f"## [{self.old}] - 2030-01-02", text)
        self.assertNotIn("Unreleased\n\n## [Unreleased]", text)
        self.assertEqual(self.bump(self.old)[0], 1)  # now already dated, so equal is refused
        self.assertIn("Cargo.toml", self.bump("9.9.9", "--dry-run")[1])

    def test_site_option_moves_download_links(self):
        code, out, err = self.bump("9.8.7", "--site")
        self.assertEqual(code, 0, err)
        readme = (self.root / "README.md").read_text()
        page = (self.root / "website/index.html").read_text()
        self.assertIn("downloads/secblitz-9.8.7-windows-x64-setup.exe", readme)
        self.assertIn("downloads/secblitz-9.8.7-windows-x64-setup.exe", page)
        self.assertIn("version-9.8.7-", readme)
        self.assertIn("Version 9.8.7", page)
        self.assertNotIn(f"secblitz-{self.shown}-windows", readme + page)

    def test_site_only_changes_just_the_site_files(self):
        before = self.snapshot()
        self.assertEqual(self.bump("9.8.7", "--site-only")[0], 1)  # not the Cargo version
        self.assertEqual(before, self.snapshot())
        code, out, err = self.bump(self.old, "--site-only")
        self.assertEqual(code, 0, err)
        after = self.snapshot()
        changed = {rel for rel in FILES if before[rel] != after[rel]}
        self.assertEqual(changed, {"README.md", "website/index.html"})
        self.assertIn(f"downloads/secblitz-{self.old}-windows-x64-setup.exe", after["README.md"].decode())

    def test_failed_plan_writes_nothing(self):
        (self.root / "assets/secblitz.rc").write_text("1 VERSIONINFO\n")
        before = self.snapshot()
        self.assertEqual(self.bump("9.8.7")[0], 1)
        self.assertEqual(before, self.snapshot())

    def test_release_notes_need_dated_section_with_entries(self):
        text = (self.root / "CHANGELOG.md").read_text()
        with self.assertRaises(ValueError):
            notes.section(text, self.old)  # still Unreleased
        with self.assertRaises(ValueError):
            notes.section(text, "9.9.9")
        self.assertIn("### Added", notes.section(text, "0.7.0"))
        self.assertNotIn("Six one", notes.section(text, "0.7.0"))


if __name__ == "__main__":
    unittest.main()
