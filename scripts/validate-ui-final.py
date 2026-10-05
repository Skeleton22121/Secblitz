"""Final, narrowly scoped Windows UI freeze/build/evidence workflow."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import zipfile

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"target/windows-v060-ui-final"
VM="4b70288b-b64d-4796-a725-006da3162d0f"
def sha(path):
    h=hashlib.sha256()
    with path.open("rb") as stream:
        for part in iter(lambda:stream.read(1048576),b""):h.update(part)
    return h.hexdigest()
def tree(root):return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob("*")) if p.is_file()}
def save(path,obj):path.write_text(json.dumps(obj,indent=2)+"\n")
mode=sys.argv[1]
if mode=="freeze":
    OUT.mkdir(exist_ok=False)
    paths=[ROOT/p for p in ["Cargo.toml","Cargo.lock","build.rs"]]
    for directory in ["src","assets","docs","scripts"]:
        paths.extend(p for p in (ROOT/directory).rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    before={str(p.relative_to(ROOT)):sha(p) for p in sorted(paths)}
    for name in before:
        dest=OUT/"source"/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(ROOT/name,dest)
    frozen=tree(OUT/"source")
    assert before==frozen=={n:sha(ROOT/n) for n in before},"Source changed during freeze"
    digest=hashlib.sha256(json.dumps(frozen,sort_keys=True,separators=(",",":")).encode()).hexdigest()
    save(OUT/"source-hashes.json",{"version":"0.6.0 UNRELEASED","sha256":frozen,"tree_sha256":digest,"stable":True})
    print(digest)
elif mode=="build":
    env=dict(os.environ,CARGO_TARGET_DIR=str(OUT/"build"))
    common=["--locked","--offline","--manifest-path",str(OUT/"source/Cargo.toml"),"--target","x86_64-pc-windows-gnu","--bin","secblitz"]
    for name,args in [("cli-build",["test",*common,"--no-run","--message-format=json"]),("release-build",["build",*common,"--release"])]:
        result=subprocess.run(["cargo",*args],env=env,capture_output=True,text=True)
        (OUT/(name+".jsonl")).write_text(result.stdout);(OUT/(name+".log")).write_text(result.stderr)
        print(result.stderr,flush=True)
        if result.returncode:
            for line in result.stdout.splitlines():
                try:
                    msg=json.loads(line)
                    if msg.get("reason")=="compiler-message":print(msg["message"].get("rendered",""))
                except ValueError: print(line)
            raise SystemExit(result.returncode)
    bins=OUT/"bins";bins.mkdir(exist_ok=False)
    shutil.copy2(OUT/"build/x86_64-pc-windows-gnu/release/secblitz.exe",bins/"secblitz.exe")
    tests=[]
    for line in (OUT/"cli-build.jsonl").read_text().splitlines():
        msg=json.loads(line)
        if msg.get("reason")=="compiler-artifact" and msg.get("executable") and msg["profile"]["test"] and msg["target"]["name"]=="secblitz" and msg["target"]["kind"]==["bin"]:tests.append(Path(msg["executable"]))
    assert len(tests)==1
    shutil.copy2(tests[0],bins/"native-cli.exe")
    save(OUT/"binary-hashes.json",tree(bins));print(json.dumps(tree(bins),indent=2))
elif mode=="guest-script":
    text=(ROOT/"scripts/validate-foundation-guest.ps1").read_text()
    text=text.replace("SecblitzFoundationValidation20261003","SecblitzV060UiFinal").replace("'candidate-b'","'candidate-final'")
    text=text.replace("'prepare','stage'","'prepare','cli','stage'")
    text=text.replace("@('secblitz.exe','native-lib.exe','native-cli.exe')","@('secblitz.exe','native-cli.exe')")
    text=text.replace("if($Phase -eq 'stop-fixture')","if($Phase -eq 'cli') { $null=Run \"native-cli\" \"$bin\\native-cli.exe\" @('--test-threads=1') 240 } elseif($Phase -eq 'stop-fixture')")
    (OUT/"validate-ui-final-guest.ps1").write_text(text)
elif mode=="unpack":
    archive=OUT/sys.argv[2];dest=OUT/archive.stem;dest.mkdir(exist_ok=False)
    with zipfile.ZipFile(archive) as z:
        for name in z.namelist():
            path=(dest/name.replace("\\","/")).resolve();assert path.is_relative_to(dest.resolve())
            if name.endswith("/"):path.mkdir(parents=True,exist_ok=True)
            else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(z.read(name))
    for p in sorted(dest.glob("*.status.json")):
        status=json.loads(p.read_text(encoding="utf-8-sig"))
        if "Events" in status:status["KeyPairs"]=len(status.pop("Events"))
        print(p.name,json.dumps(status,indent=2))
    for p in sorted(dest.glob("*.FAILURE.txt")):print(p.name,p.read_text(encoding="utf-8-sig"))
elif mode in ("vm-state","confirm-off"):
    for _ in range(90 if mode=="confirm-off" else 1):
        text=subprocess.run(["VBoxManage","showvminfo",VM,"--machinereadable"],check=True,capture_output=True,text=True).stdout
        assert all(f'nic{i}="none"' in text for i in range(1,9))
        if mode=="vm-state" or 'VMState="poweroff"' in text:
            (OUT/("vm-final.txt" if mode=="confirm-off" else "vm-before.txt")).write_text(text)
            print("\n".join(s for s in text.splitlines() if s.startswith(("VMState","nic"))));break
        time.sleep(1)
    else:raise SystemExit("Clean shutdown not confirmed; no force-off")
elif mode=="finalize":
    folder=OUT/"final-results";read=lambda p:json.loads(p.read_text(encoding="utf-8-sig"))
    assert read(folder/"controls-before.json")==read(folder/"controls-final.json")
    assert read(folder/"journal-before.json")==read(folder/"journal-backup.json")==read(folder/"journal-restored.json")
    assert read(folder/"validation-processes-final.json")==[]
    assert 'VMState="poweroff"' in (OUT/"vm-final.txt").read_text()
    source=read(OUT/"source-hashes.json");assert tree(OUT/"source")==source["sha256"]
    binaries=read(OUT/"binary-hashes.json");assert tree(OUT/"bins")==binaries
    drift=[n for n,h in source["sha256"].items() if (n.startswith(("src/","assets/")) or n in ["Cargo.toml","Cargo.lock","build.rs"]) and sha(ROOT/n)!=h]
    statuses={p.stem:read(p) for p in folder.glob("*.status.json")}
    assert all(s.get("ExitCode")==0 for s in statuses.values()),statuses
    modes=[read(p) for p in sorted(folder.glob("console-mode-*.json"))];assert modes and all(m["equal"] and m["child_exit"]==0 for m in modes)
    text=(folder/"native-cli.out").read_text(encoding="utf-8-sig")
    counts=re.search(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; .*; (\d+) filtered out",text);assert counts
    subprocess.run([sys.executable,str(ROOT/"scripts/render-conpty-captures.py"),str(folder)],check=True)
    shutil.copytree(folder/"screenshots",OUT/"screenshots")
    summary={"version":"0.6.0 UNRELEASED","source_tree_sha256":source["tree_sha256"],"binaries_sha256":binaries,"app_source_drift":drift,
             "native_cli":dict(zip(["passed","failed","ignored","filtered_out"],map(int,counts.groups()))),"statuses":statuses,"console_modes":modes,
             "all_18_controls_unchanged":True,"journals_bytes_acl_attributes_restored":True,"vm_uuid":VM,"vm_poweroff_all_nics_none":True,
             "baseline_sha256":{n:sha(folder/n) for n in ["controls-before.json","controls-final.json","journal-before.json","journal-backup.json","journal-restored.json"]},
             "snapshots":read(OUT/"screenshots/manifest.json"),"evidence_zip_sha256":sha(OUT/"final-results.zip"),
             "harness_sha256":{n:sha(ROOT/"scripts"/n) for n in ["validate-ui-final.py","test-ui-final-conpty.ps1","native-conpty-driver.cs","native-console-launcher.rs","render-conpty-captures.py"]},
             "launcher_sha256":sha(OUT/"console-launcher.exe"),
             "limitations":["Prior library native acceptance retained; library suite not rerun","No actual repair, update installation, production installation, signing or publishing","Windows 10/Home/Pro unavailable"]}
    save(OUT/"summary.json",summary)
    print(json.dumps({k:summary[k] for k in ["source_tree_sha256","binaries_sha256","native_cli","console_modes","baseline_sha256","app_source_drift"]},indent=2))
else:raise SystemExit("Unknown mode")
