"""Freeze source and record native validation evidence; no credentials or VM control."""
import hashlib
import json
import pathlib
import os
import shutil
import subprocess
import sys
import time
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / "target/windows-foundation-validation"

def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def inputs():
    paths = [ROOT / x for x in ("Cargo.toml", "Cargo.lock", "build.rs")]
    for name in ("src", "assets", "tests", "docs"):
        paths.extend(p for p in (ROOT / name).rglob("*") if p.is_file())
    return sorted(paths)

mode = sys.argv[1]
if mode in ("shutdown-clone", "confirm-off"):
    vm = "4b70288b-b64d-4796-a725-006da3162d0f"
    if mode == "shutdown-clone":
        subprocess.run(["VBoxManage", "controlvm", vm, "acpipowerbutton"], check=True)
    for _ in range(60):
        result = subprocess.run(["VBoxManage", "showvminfo", vm, "--machinereadable"], check=True, capture_output=True, text=True)
        if 'VMState="poweroff"' in result.stdout:
            assert all(f'nic{i}="none"' in result.stdout for i in range(1,9))
            (OUT / "vm-final.txt").write_text(result.stdout)
            print("Authorized UI clone powered off; NIC1 through NIC8 none")
            break
        time.sleep(1)
    else:
        raise SystemExit("Clean shutdown not confirmed; do not force power off")
elif mode == "fork-native":
    base = OUT / sys.argv[2]
    dest = OUT / sys.argv[3]
    dest.mkdir(exist_ok=False)
    shutil.copytree(base / "source", dest / "source")
    for name in ["src/diagnostics/windows.rs", "src/diagnostics/common.ps1"]:
        shutil.copy2(ROOT / name, dest / "source" / name)
    frozen = {str(p.relative_to(dest / "source")): digest(p) for p in sorted((dest / "source").rglob("*")) if p.is_file()}
    record = {"base": sys.argv[2], "native_changes_only": True, "sha256": frozen,
              "source_tree_sha256": hashlib.sha256(json.dumps(frozen, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
    (dest / "source-hashes.json").write_text(json.dumps(record, indent=2) + "\n")
    print(record["source_tree_sha256"])
elif mode == "freeze":
    name = sys.argv[2]
    assert name.replace("-", "").isalnum()
    dest = OUT / name
    dest.mkdir(parents=True, exist_ok=False)
    before = {str(p.relative_to(ROOT)): digest(p) for p in inputs()}
    for name in before:
        target = dest / "source" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, target)
    frozen = {name: digest(dest / "source" / name) for name in before}
    after = {str(p.relative_to(ROOT)): digest(p) for p in inputs()}
    record = {"captured_at": time.time(), "sha256": frozen,
              "stable_during_copy": before == frozen == after,
              "source_tree_sha256": hashlib.sha256(json.dumps(frozen, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
    (dest / "source-hashes.json").write_text(json.dumps(record, indent=2) + "\n")
    if not record["stable_during_copy"]:
        raise SystemExit("Concurrent source change; preserve this capture but freeze again before building")
    print(json.dumps({k: v for k, v in record.items() if k != "sha256"}, indent=2))
elif mode == "build":
    dest = OUT / sys.argv[2]
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(dest / "build")
    common = ["--locked", "--offline", "--manifest-path", str(dest / "source/Cargo.toml"), "--target", "x86_64-pc-windows-gnu"]
    for label, command in [
        ("test-build", ["cargo", "test", *common, "--no-run", "--message-format=json"]),
        ("candidate-build", ["cargo", "build", *common, "--bin", "secblitz"]),
    ]:
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        (dest / (label + ".jsonl")).write_text(result.stdout)
        (dest / (label + ".log")).write_text(result.stderr)
        print(result.stderr, flush=True)
        if result.returncode:
            for line in result.stdout.splitlines():
                try:
                    message = json.loads(line)
                    if message.get("reason") == "compiler-message":
                        print(message["message"].get("rendered", ""), flush=True)
                except ValueError:
                    print(line, flush=True)
            raise SystemExit(result.returncode)
elif mode == "seal":
    dest = OUT / sys.argv[2]
    build = dest / "build/x86_64-pc-windows-gnu/debug"
    bins = dest / "bins"
    bins.mkdir(exist_ok=False)
    paths = {"secblitz.exe": build / "secblitz.exe"}
    # Rust test metadata is the authority for lib/bin selection.
    for line in (dest / "test-build.jsonl").read_text().splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        target = message["target"]
        if message.get("profile", {}).get("test") and target["name"] == "secblitz":
            if target["kind"] == ["lib"]:
                paths["native-lib.exe"] = pathlib.Path(message["executable"])
            elif target["kind"] == ["bin"]:
                paths["native-cli.exe"] = pathlib.Path(message["executable"])
    assert set(paths) == {"secblitz.exe", "native-lib.exe", "native-cli.exe"}
    hashes = {}
    for name, source in paths.items():
        shutil.copy2(source, bins / name)
        hashes[name] = digest(bins / name)
    (dest / "binary-hashes.json").write_text(json.dumps(hashes, indent=2) + "\n")
    print(json.dumps(hashes, indent=2))
elif mode == "unpack":
    archive = OUT / sys.argv[2]
    dest = OUT / archive.stem
    dest.mkdir(exist_ok=False)
    with zipfile.ZipFile(archive) as z:
        for name in z.namelist():
            target = (dest / name.replace("\\", "/")).resolve()
            assert target.is_relative_to(dest.resolve())
            if name.endswith("/"):
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(z.read(name))
    for path in sorted(dest.rglob("*.status.json")):
        print(path.name, path.read_text(encoding="utf-8-sig"))
    for path in sorted(dest.rglob("*.FAILURE.txt")):
        print(path.name, path.read_text(encoding="utf-8-sig"))
elif mode == "summarize":
    folder = OUT / sys.argv[2]
    candidate = sys.argv[3]
    report = json.loads((folder / (candidate + "-diagnostics.out")).read_text(encoding="utf-8-sig"))
    (folder / (candidate + "-diagnostics.pretty.json")).write_text(json.dumps(report, indent=2) + "\n")
    def unknowns(value, prefix=""):
        if isinstance(value, dict):
            if value.get("state") == "Unknown":
                return [prefix + "=" + str(value.get("value"))]
            return [s for key, child in value.items() for s in unknowns(child, prefix + "." + key)]
        if isinstance(value, list):
            return [s for i, child in enumerate(value) for s in unknowns(child, prefix + "." + str(i))]
        return []
    for p in report["probes"]:
        print(p["id"], p["status"], "failure=" + str(p["failure"]), "unknown=" + ",".join(unknowns(p["evidence"])))
    print("Coverage:", report["coverage"])
elif mode == "finalize":
    folder = OUT / "final-results"
    candidate = OUT / "candidate-c"
    def read_json(path):
        return json.loads(path.read_text(encoding="utf-8-sig"))
    before = read_json(folder / "journal-before.json")
    restored = read_json(folder / "journal-restored.json")
    assert before == restored
    assert read_json(folder / "controls-before.json") == read_json(folder / "controls-final.json")
    assert read_json(folder / "validation-processes-final.json") == []
    vm = (OUT / "vm-final.txt").read_text()
    assert 'VMState="poweroff"' in vm and all(f'nic{i}="none"' in vm for i in range(1,9))
    hashes = read_json(candidate / "binary-hashes.json")
    assert all(digest(candidate / "bins" / name) == h for name, h in hashes.items())
    source = read_json(candidate / "source-hashes.json")
    assert all(digest(candidate / "source" / name) == h for name, h in source["sha256"].items())
    report = read_json(folder / "candidate-c-diagnostics.out")
    statuses = {p.name: read_json(p) for p in folder.glob("candidate-c-*.status.json")}
    summary = {
        "vm_uuid": "4b70288b-b64d-4796-a725-006da3162d0f", "vm_poweroff_all_nics_none": True,
        "environment": read_json(folder / "environment.json"),
        "source_tree_sha256": source["source_tree_sha256"], "binaries_sha256": hashes,
        "baseline_files_sha256": {p["Path"]: p["SHA256"] for p in before if not p["Directory"]},
        "evidence_sha256": {name: digest(folder / name) for name in ["controls-before.json", "controls-final.json", "journal-before.json", "journal-backup.json", "journal-restored.json", "preservation.json", "check-health-durable-evidence.json"]},
        "validation_scripts_sha256": {name: digest(ROOT / "scripts" / name) for name in ["validate-foundation.py", "validate-foundation-guest.ps1", "test-diagnostics-script.ps1"]},
        "full_results_zip_sha256": digest(OUT / "final-results.zip"),
        "all_18_controls_unchanged": True, "journal_bytes_acl_attributes_restored": True,
        "native_cli": {"passed": 82, "ignored": 2},
        "native_library_excluding_engine": {"passed": 165, "ignored": 24, "filtered_out": 73},
        "selected_ignored_native_tests_passed": 11,
        "full_library_blocked": ["15-minute timeout in every_cow_snapshot_byte_prefix_reopens_and_undo_uses_only_committed_originals", "10-minute timeout in bounded rerun, last output legacy_twelve_control_schema_one_wal_replays_with_extended_catalog"],
        "diagnostics_coverage": report["coverage"],
        "diagnostics_probes": [{"id": p["id"], "status": p["status"], "failure": p["failure"]} for p in report["probes"]],
        "check_health": "Preflight blocked; durable pending step has no baseline/process/exit/evidence. Default UTC window was closed; CLI generic error does not identify first failing gate. No DISM execution claimed.",
        "missing_external_coverage": ["Windows 10", "Windows Home", "Windows Pro", "installed-image health tests", "real patch/update installation"],
        "statuses": statuses,
    }
    (OUT / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: summary[k] for k in ["source_tree_sha256", "binaries_sha256", "evidence_sha256", "full_results_zip_sha256", "diagnostics_coverage"]}, indent=2))
else:
    raise SystemExit("Expected freeze or seal")
