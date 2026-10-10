#!/usr/bin/env python3
"""Write the parts list (a CycloneDX 1.5 SBOM) of one Secblitz build target.

    make-sbom.py --target x86_64-pc-windows-msvc --output secblitz-0.12.0-windows-x64.cdx.json

Lists the Rust crates in the normal dependency graph of the secblitz package for
that target. Crates used only for tests or while building are left out. Each
crate carries its version, license, the SHA-256 recorded in Cargo.lock and where
it comes from. The output is the same every time unless --timestamp is given.
Runs `cargo metadata --locked`, so it needs cargo and a Cargo.lock that is up to
date. --metadata and --lock read saved files instead (used by the tests).
Standard library only.
"""
import argparse
import json
import re
import subprocess
import sys
import tomllib
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROOT_PACKAGE = "secblitz"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SPARSE_CRATES_IO = "sparse+https://index.crates.io/"
NAMESPACE = uuid.UUID("5b0f2f0e-3c6b-4d0e-9a53-6d5d1f0c6c11")


class SbomError(Exception):
    pass


def purl(name, version, source=None):
    text = f"pkg:cargo/{name}@{version}"
    if source and source.startswith("git+"):
        text += "?vcs_url=" + source[4:].split("#")[0].replace("?", "%3F")
    return text


def license_expression(raw):
    if not raw:
        return None
    text = re.sub(r"\s*/\s*", " OR ", raw.strip())
    return re.sub(r"\s+", " ", text)


def local_path(metadata, package):
    manifest = package["manifest_path"].replace("\\", "/")
    base = metadata["workspace_root"].replace("\\", "/").rstrip("/") + "/"
    if manifest.startswith(base):
        manifest = manifest[len(base):]
    return manifest.rsplit("/", 1)[0] if "/" in manifest else "."


def lock_checksums(lock_text):
    sums = {}
    for package in tomllib.loads(lock_text).get("package", []):
        if "checksum" in package:
            sums[(package["name"], package["version"], package.get("source"))] = package["checksum"]
    return sums


def normal_closure(metadata):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    if root is None or root not in nodes:
        raise SbomError("The metadata has no root package.")
    seen, edges, queue = {root}, {}, [root]
    while queue:
        current = queue.pop()
        children = set()
        for dep in nodes[current]["deps"]:
            kinds = dep.get("dep_kinds") or [{"kind": None}]
            if any(kind.get("kind") is None for kind in kinds):
                children.add(dep["pkg"])
        edges[current] = children
        for child in children:
            if child not in seen:
                seen.add(child)
                queue.append(child)
    return root, seen, edges


def build(metadata, lock_text, target, timestamp=None):
    packages = {package["id"]: package for package in metadata["packages"]}
    root, included, edges = normal_closure(metadata)
    if packages[root]["name"] != ROOT_PACKAGE:
        raise SbomError(f"The root package is {packages[root]['name']}, not {ROOT_PACKAGE}.")
    sums = lock_checksums(lock_text)

    def ref(package_id):
        package = packages[package_id]
        return purl(package["name"], package["version"], package.get("source"))

    components = []
    for package_id in included - {root}:
        package = packages[package_id]
        source = package.get("source")
        component = {
            "type": "library",
            "bom-ref": ref(package_id),
            "name": package["name"],
            "version": package["version"],
            "purl": ref(package_id),
        }
        expression = license_expression(package.get("license"))
        if expression:
            component["licenses"] = [{"expression": expression}]
        checksum = sums.get((package["name"], package["version"], source))
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        if source in (CRATES_IO, SPARSE_CRATES_IO):
            component["externalReferences"] = [{
                "type": "distribution",
                "url": f"https://crates.io/api/v1/crates/{package['name']}/{package['version']}/download",
            }]
            component["properties"] = [{"name": "cargo:source", "value": "crates.io"}]
        elif source and source.startswith("git+"):
            component["externalReferences"] = [{"type": "vcs", "url": source[4:].split("#")[0].split("?")[0]}]
            component["properties"] = [{"name": "cargo:source", "value": source}]
        elif source:
            component["properties"] = [{"name": "cargo:source", "value": source}]
        else:
            component["properties"] = [{"name": "cargo:source", "value": "path:" + local_path(metadata, package)}]
        components.append(component)
    components.sort(key=lambda item: (item["name"], item["version"], item["bom-ref"]))
    refs = [item["bom-ref"] for item in components] + [ref(root)]
    if len(set(refs)) != len(refs):
        raise SbomError("Two packages share a name and version, so their parts list references would clash.")

    root_package = packages[root]
    root_ref = ref(root)
    root_component = {
        "type": "application",
        "bom-ref": root_ref,
        "name": root_package["name"],
        "version": root_package["version"],
        "purl": root_ref,
    }
    expression = license_expression(root_package.get("license"))
    if expression:
        root_component["licenses"] = [{"expression": expression}]
    if root_package.get("repository"):
        root_component["externalReferences"] = [{"type": "vcs", "url": root_package["repository"]}]

    dependencies = [
        {"ref": ref(package_id), "dependsOn": sorted(ref(child) for child in edges.get(package_id, ()))}
        for package_id in included
    ]
    dependencies.sort(key=lambda item: item["ref"])

    meta = {
        "component": root_component,
        "tools": {"components": [{"type": "application", "name": "make-sbom.py"}]},
        "properties": [{"name": "cargo:target", "value": target}],
    }
    if timestamp:
        meta["timestamp"] = timestamp
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:" + str(uuid.uuid5(NAMESPACE, f"{root_package['name']}@{root_package['version']}/{target}")),
        "version": 1,
        "metadata": meta,
        "components": components,
        "dependencies": dependencies,
    }


def render(document):
    return json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--target", required=True, help="Rust target, for example x86_64-pc-windows-msvc")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timestamp", help="optional ISO 8601 time to record; left out by default so the file is repeatable")
    parser.add_argument("--metadata", type=Path, help="saved `cargo metadata` output to read instead of running cargo")
    parser.add_argument("--lock", type=Path, default=ROOT / "Cargo.lock")
    args = parser.parse_args(argv)
    try:
        if args.metadata:
            metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
        else:
            done = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", args.target],
                cwd=ROOT, capture_output=True, text=True)
            if done.returncode:
                raise SbomError("cargo metadata failed: " + done.stderr.strip())
            metadata = json.loads(done.stdout)
        document = build(metadata, args.lock.read_text(encoding="utf-8"), args.target, args.timestamp)
    except (SbomError, OSError, ValueError, KeyError) as error:
        print(f"make-sbom.py: {error}", file=sys.stderr)
        return 1
    args.output.write_text(render(document), encoding="utf-8", newline="\n")
    print(f"{args.output}: {len(document['components'])} components")
    return 0


if __name__ == "__main__":
    sys.exit(main())
