#!/usr/bin/env python3
"""Compile the actual module without cfg(test); optionally cross-link Windows.

Uses Cargo's artifact messages for exact dependency paths, never guesses rlib
hashes or changes lib.rs. All artifacts stay in the caller's temporary target.
--unit-tests independently builds the normal integration harness with cfg(test).
The Windows executable is not run. No installer, network workflow, or VM runs.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=["x86_64-pc-windows-gnu"])
    parser.add_argument("--unit-tests", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output_root = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if not output_root.is_relative_to(Path("/tmp/opencode")):
        raise SystemExit("Production probe requires a /tmp/opencode target directory")

    command = ["cargo", "build", "--locked", "--lib", "--message-format=json"]
    if args.target:
        command += ["--target", args.target]
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, check=True)
    required = {"anyhow", "serde", "uuid"}
    if args.unit_tests:
        required.add("serde_json")
    dependencies = {}
    library_paths = set()
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact":
            continue
        # Cross compilation still loads derive macros on the Linux host.
        # Preserve Cargo's host macro search path as well as target rlibs.
        for filename in message["filenames"]:
            path = Path(filename)
            if path.suffix in {".rlib", ".so", ".dylib", ".dll"}:
                library_paths.add(str(path.parent))
        name = message["target"]["name"]
        if name not in required:
            continue
        for filename in message["filenames"]:
            path = Path(filename)
            if path.suffix == ".rlib" and (not args.target or args.target in path.parts):
                dependencies[name] = path
    if set(dependencies) != required:
        raise SystemExit(f"Missing exact Cargo dependency artifacts: {dependencies}")

    suffix = "windows.exe" if args.target else "host"
    mode = "unit-tests" if args.unit_tests else "production"
    source = "tests/app_updates.rs" if args.unit_tests else "tests/fixtures/app_updates_production.rs"
    output = output_root / f"app-updates-{mode}-{suffix}"
    command = [
        os.environ.get("RUSTC", "rustc"),
        "--edition=2021",
        "--crate-name=app_updates_production_probe",
        str(root / source),
        "-o", str(output),
    ]
    if args.unit_tests:
        command.append("--test")
    for directory in sorted(library_paths):
        command += ["-L", f"dependency={directory}"]
    for name, path in sorted(dependencies.items()):
        command += ["--extern", f"{name}={path}"]
    if args.target:
        command += ["--target", args.target]
        linker = os.environ.get("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER")
        if linker:
            command += ["-C", f"linker={linker}"]
    subprocess.run(command, cwd=root, check=True)

    # The historical hash is a distinctive canary. The production binary must
    # not embed the cfg(test)-only fixture, even when compiled without stripping.
    if not args.unit_tests:
        fixture = json.loads((root / "src/app_updates/fixtures/obsolete-vscode.json").read_text())
        if fixture["installer_sha256"].encode() in output.read_bytes():
            raise SystemExit("Historical catalog fixture leaked into production binary")
    if not args.target:
        subprocess.run([str(output)], cwd=root, check=True)
    if args.unit_tests:
        print(f"Unit-test harness {'cross-linked' if args.target else 'passed'} ({args.target or 'host'})")
    else:
        print(f"Production gate passed ({args.target or 'host'}); historical fixture absent")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
