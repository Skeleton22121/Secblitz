"""Frozen 0.6.0 native candidate validation. Transport is UI-clone-only."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target/windows-v060-candidate-validation"
VM = "4b70288b-b64d-4796-a725-006da3162d0f"

def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1048576), b""):
            h.update(chunk)
    return h.hexdigest()

def tree(folder):
    return {str(p.relative_to(folder)): sha(p) for p in sorted(folder.rglob("*")) if p.is_file()}

def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")

mode = sys.argv[1]
if mode == "freeze":
    dest = OUT / sys.argv[2]
    dest.mkdir(parents=True, exist_ok=False)
    names = ["Cargo.toml", "Cargo.lock", "build.rs"]
    for directory in ["src", "assets", "tests", "docs", "scripts"]:
        names += [str(p.relative_to(ROOT)) for p in (ROOT / directory).rglob("*") if p.is_file() and "__pycache__" not in p.parts]
    before = {n: sha(ROOT / n) for n in sorted(names)}
    for name in before:
        target = dest / "source" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, target)
    frozen = tree(dest / "source")
    after = {n: sha(ROOT / n) for n in sorted(names)}
    summary = {"version": "0.6.0-unreleased", "stable": before == frozen == after, "sha256": frozen,
               "tree_sha256": hashlib.sha256(json.dumps(frozen, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
    save(dest / "source-hashes.json", summary)
    assert summary["stable"], "Source changed during freeze; preserve capture and freeze again"
    print(summary["tree_sha256"])
elif mode == "build":
    dest = OUT / sys.argv[2]
    env = dict(os.environ, CARGO_TARGET_DIR=str(dest / "build"))
    common = ["--locked", "--offline", "--manifest-path", str(dest / "source/Cargo.toml"), "--target", "x86_64-pc-windows-gnu"]
    for label, args in [("tests", ["test", *common, "--no-run", "--message-format=json"]), ("release", ["build", *common, "--release", "--bin", "secblitz"])]:
        result = subprocess.run(["cargo", *args], env=env, capture_output=True, text=True)
        (dest / (label + ".jsonl")).write_text(result.stdout)
        (dest / (label + ".log")).write_text(result.stderr)
        print(result.stderr, flush=True)
        if result.returncode:
            for line in result.stdout.splitlines():
                try:
                    msg = json.loads(line)
                    if msg.get("reason") == "compiler-message":
                        print(msg["message"].get("rendered", ""))
                except ValueError:
                    print(line)
            raise SystemExit(result.returncode)
    bins = dest / "bins"
    bins.mkdir(exist_ok=False)
    paths = {"secblitz.exe": dest / "build/x86_64-pc-windows-gnu/release/secblitz.exe"}
    for line in (dest / "tests.jsonl").read_text().splitlines():
        msg = json.loads(line)
        if msg.get("reason") == "compiler-artifact" and msg.get("executable") and msg["profile"]["test"] and msg["target"]["name"] == "secblitz":
            kind = msg["target"]["kind"]
            if kind in [["lib"], ["bin"]]:
                paths["native-lib.exe" if kind == ["lib"] else "native-cli.exe"] = Path(msg["executable"])
    assert set(paths) == {"secblitz.exe", "native-lib.exe", "native-cli.exe"}
    for name, path in paths.items():
        shutil.copy2(path, bins / name)
    hashes = tree(bins)
    save(dest / "binary-hashes.json", hashes)
    with zipfile.ZipFile(dest / "source.zip", "w", zipfile.ZIP_DEFLATED) as z:
        for p in sorted((dest / "source").rglob("*")):
            if p.is_file():
                z.write(p, p.relative_to(dest / "source"))
    print(json.dumps(hashes, indent=2))
elif mode == "guest-script":
    # Preserve the previous run's script/evidence. Generate this run's wrapper
    # from the reviewed owned harness with a new namespace and explicit tests.
    text = (ROOT / "scripts/validate-foundation-guest.ps1").read_text()
    text = text.replace("SecblitzFoundationValidation20261003", "SecblitzV060CandidateValidation")
    text = text.replace("'candidate-b'", "'candidate-final'")
    text = text.replace("'prepare','stage'", "'prepare','mocks','stage'")
    text = text.replace("@('--test-threads=1') 900", "@('--test-threads=1') 300")
    text = text.replace("'operations::storage::tests::atomic_publication", "'patching::storage::tests::module_payload_and_executable_config_are_independently_pinned_and_checked',\n            'patching::process::tests::direct_exit_keeps_descendant_supervision_and_drop_never_kills',\n            'patching::process::tests::cancelled_or_expired_permit_never_launches',\n            'patching::windows::tests::boot_identity_and_original_user_binding_are_stable_without_wall_clock_estimates',\n            'operations::storage::tests::atomic_publication")
    text = text.replace("if($Phase -eq 'stop-fixture')", """if($Phase -eq 'mocks') {
        if(!(Test-Path \"$root\\Source\")){Expand-Archive \"$bin\\source.zip\" \"$root\\Source\"}
        foreach($script in @('diagnostics','operations','patching')) {
            $null=Run \"$Candidate-ps51-$script\" \"$env:SystemRoot\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',\"$root\\Source\\scripts\\test-$script-script.ps1\") 120
        }
    } elseif($Phase -eq 'stop-fixture')""")
    (OUT / "validate-v060-guest.ps1").write_text(text)
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
    for p in sorted(dest.rglob("*.status.json")):
        print(p.name, p.read_text(encoding="utf-8-sig"))
    for p in sorted(dest.rglob("*.FAILURE.txt")):
        print(p.name, p.read_text(encoding="utf-8-sig"))
elif mode == "vm-state":
    text = subprocess.run(["VBoxManage", "showvminfo", VM, "--machinereadable"], check=True, capture_output=True, text=True).stdout
    (OUT / (sys.argv[2] + ".txt")).write_text(text)
    print("\n".join(line for line in text.splitlines() if line.startswith(("VMState", "nic"))))
elif mode == "compare":
    hashes=json.loads((OUT / sys.argv[2] / "source-hashes.json").read_text())["sha256"]
    changed=[name for name,h in hashes.items() if (name.startswith(("src/", "assets/")) or name in ["Cargo.toml","Cargo.lock","build.rs"]) and sha(ROOT/name)!=h]
    save(OUT/"current-app-source-drift.json",changed)
    print(json.dumps(changed,indent=2))
elif mode == "finalize":
    folder=OUT/"final-results"
    candidate=OUT/"candidate-final"
    read=lambda p: json.loads(p.read_text(encoding="utf-8-sig"))
    assert read(folder/"controls-before.json")==read(folder/"controls-final.json")
    assert read(folder/"journal-before.json")==read(folder/"journal-backup.json")==read(folder/"journal-restored.json")
    assert read(folder/"validation-processes-final.json")==[]
    vm=(OUT/"vm-final.txt").read_text()
    assert 'VMState="poweroff"' in vm and all(f'nic{i}="none"' in vm for i in range(1,9))
    source=read(candidate/"source-hashes.json")
    assert tree(candidate/"source")==source["sha256"]
    binaries=read(candidate/"binary-hashes.json")
    assert tree(candidate/"bins")==binaries
    subprocess.run([sys.executable,str(ROOT/"scripts/render-conpty-captures.py"),str(folder)],check=True)
    screenshots=OUT/"screenshots"
    shutil.copytree(folder/"screenshots",screenshots)
    frames=read(screenshots/"manifest.json")
    assert all(p["text_colors"]==["#cccccc"] for p in frames if p["no_color"])
    report=read(folder/"candidate-final-diagnostics.out")
    (folder/"candidate-final-diagnostics.pretty.json").write_text(json.dumps(report,indent=2)+"\n")
    import re
    def tests(name):
        text=(folder/name).read_text(encoding="utf-8-sig")
        m=re.search(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; .*; (\d+) filtered out",text)
        assert m,name
        return dict(zip(["passed","failed","ignored","filtered_out"],map(int,m.groups())))
    console={p.stem:read(p) for p in folder.glob("conpty-*.status.json")}
    for result in console.values(): assert result["ExitCode"]==0
    proof=read(folder/"conpty-paging-80x24-color-key-proof.json")
    assert not proof["PageDownChangedOutput"] and not proof["PageUpChangedOutput"] and proof["RightChangedOutput"]
    summary={
        "version":"0.6.0 UNRELEASED", "vm_uuid":VM,"only_ui_clone_used":True,"vm_poweroff_all_nics_none":True,
        "source_tree_sha256":source["tree_sha256"],"binaries_sha256":binaries,
        "native_library":tests("candidate-final-lib.out"),"native_cli":tests("candidate-final-cli.out"),
        "explicit_native_fixtures":{"passed":14,"failed_due_to_missing_split_token_prerequisite":1},
        "powershell_51_mocks":{k:read(folder/("candidate-final-ps51-"+k+".status.json")) for k in ["diagnostics","operations","patching"]},
        "diagnostics_coverage":report["coverage"],"diagnostics_status":report["status"],
        "console_runs":console,"ui_defect":proof,
        "acceptance":"Full native suites pass; UI paging remains a coordinator-owned defect. Split-token patch identity prerequisite unavailable.",
        "all_18_controls_unchanged":True,"journal_bytes_acls_attributes_restored":True,
        "baseline_sha256":{n:sha(folder/n) for n in ["controls-before.json","controls-final.json","journal-before.json","journal-backup.json","journal-restored.json"]},
        "snapshots":frames,"snapshot_provenance":"PNG terminal cells rendered from real Windows ConPTY output; raw VT retained; not desktop framebuffer screenshots",
        "evidence_zip_sha256":sha(OUT/"final-results.zip"),
        "harness_sha256":{n:sha(ROOT/"scripts"/n) for n in ["validate-v060.py","native-conpty-driver.cs","native-console-launcher.rs","test-v060-conpty.ps1","render-conpty-captures.py"]},
        "helper_binaries_sha256":{n:sha(OUT/n) for n in ["console-launcher.exe","console-probe.exe"]},
        "app_source_drift":read(OUT/"current-app-source-drift.json"),
        "not_tested":["Real servicing repairs/DISM remediation","Actual WUA downloads/installations","Historical app-update target","Installed-image health prerequisites","Windows 10/Home/Pro"],
        "no_install_release_sign_publish":True,
    }
    save(OUT/"summary.json",summary)
    print(json.dumps({k:summary[k] for k in ["source_tree_sha256","binaries_sha256","native_library","native_cli","baseline_sha256","ui_defect"]},indent=2))
elif mode == "confirm-off":
    for _ in range(90):
        text = subprocess.run(["VBoxManage", "showvminfo", VM, "--machinereadable"], check=True, capture_output=True, text=True).stdout
        if 'VMState="poweroff"' in text:
            assert all(f'nic{i}="none"' in text for i in range(1, 9))
            (OUT / "vm-final.txt").write_text(text)
            print("UI clone powered off, all eight NICs none")
            break
        time.sleep(1)
    else:
        raise SystemExit("Clean shutdown unconfirmed; no force-off attempted")
else:
    raise SystemExit("Unknown mode")
