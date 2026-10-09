#!/usr/bin/env python3
"""Offline tests for make-sbom.py. Fixture metadata only: no cargo, no network."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("make_sbom", Path(__file__).with_name("make-sbom.py"))
sbom = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sbom)

REG = "registry+https://github.com/rust-lang/crates.io-index"
ROOT_ID = "path+file:///work/secblitz#secblitz@1.2.3"
HASH_A, HASH_B, HASH_C = "a" * 64, "b" * 64, "c" * 64


def pid(name, version):
    return f"{REG}#{name}@{version}"


def package(name, version, license="MIT", source=REG, manifest=None, pkg_id=None):
    return dict(name=name, version=version, id=pkg_id or pid(name, version), source=source, license=license,
                manifest_path=manifest or f"/home/u/.cargo/registry/{name}-{version}/Cargo.toml")


def dep(pkg, kind=None):
    return dict(pkg=pkg, dep_kinds=[dict(kind=kind, target=None)])


def node(pkg_id, deps=()):
    return dict(id=pkg_id, deps=list(deps), dependencies=[d["pkg"] for d in deps], features=[])


def fixture():
    packages = [
        dict(name="secblitz", version="1.2.3", id=ROOT_ID, source=None, license="MIT",
             repository="https://github.com/secblitz/Secblitz", manifest_path="/work/secblitz/Cargo.toml"),
        package("serde", "1.0.0", "MIT/Apache-2.0"),
        package("serde_derive", "1.0.0", "MIT OR Apache-2.0"),
        package("tempfile", "3.0.0"),
        package("embed-resource", "3.0.0"),
        package("cc", "1.0.0"),
        package("winapi", "0.3.9"),
        package("linuxonly", "0.1.0"),
        package("vend", "0.2.0", source=None, manifest="/work/secblitz/vendor/vend/Cargo.toml",
                pkg_id="path+file:///work/secblitz/vendor/vend#0.2.0"),
        package("gitdep", "0.3.0", source="git+https://example.org/gitdep?rev=abc#abc123"),
    ]
    vend = "path+file:///work/secblitz/vendor/vend#0.2.0"
    git = packages[-1]["id"]
    nodes = [
        node(ROOT_ID, [dep(pid("serde", "1.0.0")), dep(pid("tempfile", "3.0.0"), "dev"),
                       dep(pid("embed-resource", "3.0.0"), "build"), dep(vend), dep(git)]),
        node(pid("serde", "1.0.0"), [dep(pid("serde_derive", "1.0.0")), dep(pid("cc", "1.0.0"), "build")]),
        node(pid("serde_derive", "1.0.0")),
        node(pid("tempfile", "3.0.0"), [dep(pid("winapi", "0.3.9"))]),
        node(pid("embed-resource", "3.0.0"), [dep(pid("cc", "1.0.0"))]),
        node(pid("cc", "1.0.0")),
        node(pid("winapi", "0.3.9")),
        node(vend),
        node(git),
    ]
    return dict(packages=packages, workspace_root="/work/secblitz", resolve=dict(root=ROOT_ID, nodes=nodes))


LOCK = f'''
version = 4

[[package]]
name = "secblitz"
version = "1.2.3"

[[package]]
name = "serde"
version = "1.0.0"
source = "{REG}"
checksum = "{HASH_A}"

[[package]]
name = "serde_derive"
version = "1.0.0"
source = "{REG}"
checksum = "{HASH_B}"

[[package]]
name = "gitdep"
version = "0.3.0"
source = "git+https://example.org/gitdep?rev=abc#abc123"

[[package]]
name = "tempfile"
version = "3.0.0"
source = "{REG}"
checksum = "{HASH_C}"
'''


class SbomTests(unittest.TestCase):
    def doc(self, **kw):
        return sbom.build(fixture(), LOCK, "x86_64-pc-windows-msvc", **kw)

    def names(self, doc):
        return [c["name"] for c in doc["components"]]

    def test_only_the_normal_graph_is_listed(self):
        names = self.names(self.doc())
        self.assertEqual(names, ["gitdep", "serde", "serde_derive", "vend"])
        for left_out in ("tempfile", "embed-resource", "cc", "winapi", "linuxonly", "secblitz"):
            self.assertNotIn(left_out, names)

    def test_a_crate_reached_normally_and_by_build_is_kept(self):
        data = fixture()
        root = data["resolve"]["nodes"][0]
        root["deps"].append(dep(pid("cc", "1.0.0")))
        doc = sbom.build(data, LOCK, "t")
        self.assertIn("cc", self.names(doc))
        self.assertNotIn("embed-resource", self.names(doc))

    def test_a_dependency_with_several_kinds_counts_when_one_is_normal(self):
        data = fixture()
        data["resolve"]["nodes"][0]["deps"].append(dict(pkg=pid("winapi", "0.3.9"),
                                                         dep_kinds=[dict(kind="dev", target=None), dict(kind=None, target=None)]))
        self.assertIn("winapi", self.names(sbom.build(data, LOCK, "t")))

    def test_platform_filtering_follows_the_metadata(self):
        data = fixture()
        data["resolve"]["nodes"][0]["deps"].append(dep(pid("linuxonly", "0.1.0")))
        data["resolve"]["nodes"].append(node(pid("linuxonly", "0.1.0")))
        self.assertIn("linuxonly", self.names(sbom.build(data, LOCK, "x86_64-unknown-linux-gnu")))
        self.assertNotIn("linuxonly", self.names(self.doc()))

    def test_target_is_recorded(self):
        doc = self.doc()
        self.assertEqual(doc["metadata"]["properties"], [{"name": "cargo:target", "value": "x86_64-pc-windows-msvc"}])

    def test_schema_fields_are_present(self):
        doc = self.doc()
        self.assertEqual(doc["bomFormat"], "CycloneDX")
        self.assertEqual(doc["specVersion"], "1.5")
        self.assertEqual(doc["version"], 1)
        self.assertTrue(doc["serialNumber"].startswith("urn:uuid:"))
        root = doc["metadata"]["component"]
        self.assertEqual((root["type"], root["name"], root["version"]), ("application", "secblitz", "1.2.3"))
        self.assertEqual(root["purl"], "pkg:cargo/secblitz@1.2.3")
        for c in doc["components"]:
            for field in ("type", "bom-ref", "name", "version", "purl"):
                self.assertIn(field, c)
            self.assertEqual(c["purl"].split("?")[0], f"pkg:cargo/{c['name']}@{c['version']}")

    def test_license_hash_and_source(self):
        by = {c["name"]: c for c in self.doc()["components"]}
        self.assertEqual(by["serde"]["licenses"], [{"expression": "MIT OR Apache-2.0"}])
        self.assertEqual(by["serde"]["hashes"], [{"alg": "SHA-256", "content": HASH_A}])
        self.assertEqual(by["serde"]["properties"], [{"name": "cargo:source", "value": "crates.io"}])
        self.assertEqual(by["serde"]["externalReferences"][0]["url"], "https://crates.io/api/v1/crates/serde/1.0.0/download")
        self.assertNotIn("hashes", by["gitdep"])
        self.assertEqual(by["gitdep"]["externalReferences"], [{"type": "vcs", "url": "https://example.org/gitdep"}])
        self.assertTrue(by["gitdep"]["purl"].startswith("pkg:cargo/gitdep@0.3.0?vcs_url=https://example.org/gitdep"))
        self.assertEqual(by["vend"]["properties"], [{"name": "cargo:source", "value": "path:vendor/vend"}])
        self.assertNotIn("hashes", by["vend"])

    def test_dependency_graph_has_only_listed_parts(self):
        doc = self.doc()
        refs = {c["bom-ref"] for c in doc["components"]} | {doc["metadata"]["component"]["bom-ref"]}
        self.assertEqual({d["ref"] for d in doc["dependencies"]}, refs)
        for d in doc["dependencies"]:
            self.assertTrue(set(d["dependsOn"]) <= refs)
        root = next(d for d in doc["dependencies"] if d["ref"] == "pkg:cargo/secblitz@1.2.3")
        self.assertIn("pkg:cargo/serde@1.0.0", root["dependsOn"])
        self.assertNotIn("pkg:cargo/tempfile@3.0.0", root["dependsOn"])

    def test_output_is_deterministic_and_has_no_time(self):
        first = sbom.render(self.doc())
        shuffled = fixture()
        shuffled["packages"].reverse()
        shuffled["resolve"]["nodes"].reverse()
        self.assertEqual(first, sbom.render(sbom.build(shuffled, LOCK, "x86_64-pc-windows-msvc")))
        self.assertNotIn("timestamp", first)
        self.assertTrue(first.endswith("}\n"))

    def test_timestamp_only_when_given(self):
        doc = self.doc(timestamp="2026-10-09T00:00:00Z")
        self.assertEqual(doc["metadata"]["timestamp"], "2026-10-09T00:00:00Z")

    def test_serial_differs_by_target(self):
        other = sbom.build(fixture(), LOCK, "aarch64-pc-windows-msvc")
        self.assertNotEqual(other["serialNumber"], self.doc()["serialNumber"])

    def test_missing_root_is_an_error(self):
        data = fixture()
        data["resolve"]["root"] = None
        with self.assertRaises(sbom.SbomError):
            sbom.build(data, LOCK, "t")

    def test_license_forms(self):
        self.assertEqual(sbom.license_expression("MIT/Apache-2.0"), "MIT OR Apache-2.0")
        self.assertEqual(sbom.license_expression("MIT OR  Apache-2.0"), "MIT OR Apache-2.0")
        self.assertIsNone(sbom.license_expression(None))

    def test_command_line_writes_the_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "meta.json").write_text(json.dumps(fixture()))
            (tmp / "Cargo.lock").write_text(LOCK)
            out = tmp / "out.cdx.json"
            code = sbom.main(["--target", "t", "--output", str(out), "--metadata", str(tmp / "meta.json"),
                              "--lock", str(tmp / "Cargo.lock")])
            self.assertEqual(code, 0)
            self.assertEqual(json.loads(out.read_text())["bomFormat"], "CycloneDX")


if __name__ == "__main__":
    unittest.main()
