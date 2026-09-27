"""Black-box installer acceptance with a local release transport.

Failure cases specified before implementation: corrupt checksum, mismatched
binary version, existing install preservation, paths with spaces, no PATH edits.
The test replaces transport in its process; installers use official HTTPS URLs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--artifact", default="target/installer-e2e.json")
    args = parser.parse_args()
    binary = Path(args.bin).resolve()
    root = Path(__file__).resolve().parent.parent
    version = subprocess.check_output([str(binary), "--version"], text=True).strip().split()[-1]
    evidence = {"ok": False, "scenarios": [], "platform": os.name}
    artifact = Path(args.artifact).resolve()
    try:
        with tempfile.TemporaryDirectory(prefix="bracel-install-") as temp:
            stage = Path(temp)
            destination = stage / "installation with spaces"
            env = os.environ.copy()
            env["INSTALL_FIXTURE_BINARY"] = str(binary)
            digest = hashlib.sha256(binary.read_bytes()).hexdigest()
            if os.name == "nt":
                asset = "bracel-x86_64-pc-windows-msvc.exe"
                script = stage / "invoke.ps1"
                script.write_text('''param($Installer, $Version, $Destination)
function Invoke-WebRequest { param($Uri, $OutFile)
    if ($Uri -notlike 'https://github.com/4H1R/bracel/releases/download/*') { throw 'Unexpected transport' }
    if ($Uri.EndsWith('/SHA256SUMS')) { Copy-Item -LiteralPath $env:INSTALL_FIXTURE_SUMS -Destination $OutFile }
    else { Copy-Item -LiteralPath $env:INSTALL_FIXTURE_BINARY -Destination $OutFile }
}
& $Installer -Version $Version -InstallDir $Destination -NoPath
''')
                cmd = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(script), str(root / "install.ps1"), version, str(destination)]
                installed = destination / "bracel.exe"
            else:
                import platform
                arch = "aarch64" if platform.machine() == "arm64" else "x86_64"
                target = f"{arch}-apple-darwin" if platform.system() == "Darwin" else "x86_64-unknown-linux-gnu"
                asset = f"bracel-{target}"
                fake = stage / "tools"
                fake.mkdir()
                curl = fake / "curl"
                curl.write_text('''#!/bin/sh
while [ "$#" -gt 0 ]; do
 case "$1" in
  --output) out=$2; shift 2 ;;
  https://github.com/4H1R/bracel/releases/download/*) url=$1; shift ;;
  *) shift ;;
 esac
done
case "$url" in
 */SHA256SUMS) cp "$INSTALL_FIXTURE_SUMS" "$out" ;;
 *) cp "$INSTALL_FIXTURE_BINARY" "$out" ;;
esac
''')
                curl.chmod(0o755)
                env["PATH"] = str(fake) + os.pathsep + env["PATH"]
                cmd = ["bash", str(root / "install.sh"), "--version", version, "--install-dir", str(destination)]
                installed = destination / "bracel"
            sums = stage / "SHA256SUMS"
            sums.write_text(f"{digest}  {asset}\n")
            env["INSTALL_FIXTURE_SUMS"] = str(sums)
            result = subprocess.run(cmd, env=env, capture_output=True, text=True)
            assert result.returncode == 0, (result.stdout, result.stderr)
            assert installed.read_bytes() == binary.read_bytes()
            assert subprocess.check_output([str(installed), "--version"], text=True).strip() == f"bracel {version}"
            evidence["scenarios"].append("verified install into a path containing spaces")
            sums.write_text(f"{'0' * 64}  {asset}\n")
            result = subprocess.run(cmd, env=env, capture_output=True, text=True)
            assert result.returncode != 0, result.stdout
            assert installed.read_bytes() == binary.read_bytes()
            evidence["scenarios"].append("checksum rejection preserves existing executable")
            sums.write_text(f"{digest}  {asset}\n")
            parts = version.split(".")
            wrong = ".".join(parts[:2] + [str(int(parts[2]) + 1)])
            wrong_cmd = [wrong if part == version else part for part in cmd]
            result = subprocess.run(wrong_cmd, env=env, capture_output=True, text=True)
            assert result.returncode != 0, result.stdout
            assert installed.read_bytes() == binary.read_bytes()
            evidence["scenarios"].append("unexpected binary version preserves existing executable")
            evidence["ok"] = True
    finally:
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps(evidence))


if __name__ == "__main__":
    main()
