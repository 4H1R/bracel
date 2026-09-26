"""Measure fresh, unchanged and edited builds in an isolated source snapshot.

The output directory must be new. Downloaded Cargo sources are reused, but all
compiled artifacts start empty. No files in the original project are modified.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import threading


def available_memory_gib():
    if os.name != "nt":
        return None
    import ctypes

    class MemoryStatus(ctypes.Structure):
        _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong)] + [
            (name, ctypes.c_ulonglong) for name in
            ("total", "available", "page_total", "page_available", "virtual_total", "virtual_available", "extended")]

    status = MemoryStatus()
    status.length = ctypes.sizeof(status)
    if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
        return status.available / (1024 ** 3)
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True, help="Standalone starter directory")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--profile", default="dev")
    parser.add_argument("--command", choices=["build", "check", "test"], default="build")
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    args = parser.parse_args()
    project = args.project.resolve()
    output = args.output.resolve()
    if output.exists():
        parser.error("Output already exists; use a fresh directory for a clean measurement")
    if args.jobs < 1:
        parser.error("Jobs must be positive")
    environment = os.environ.copy()
    overrides = dict(value.split("=", 1) for value in args.env)
    environment.update(overrides)
    environment["CARGO_TARGET_DIR"] = str(output / "target")
    # Empty values explicitly override wrappers from Cargo config as well as env.
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"):
        environment[name] = ""
    environment["CARGO_BUILD_BUILD_DIR"] = str(output / "target")
    source = output / "source"
    # Copy only build inputs, avoiding recursion when output lives under target/.
    source.mkdir(parents=True)
    for name in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml"):
        shutil.copy2(project / name, source / name)
    for name in ("src", "tests", "docs", ".cargo"):
        if (project / name).is_dir():
            shutil.copytree(project / name, source / name)
    command = ["cargo", args.command, "--locked", "--profile", args.profile,
               "--jobs", str(args.jobs), "--timings", "--message-format=json"]
    command += ["--no-run"] if args.command == "test" else ["--bin", "bracel-starter"]
    report = {"project": str(project), "command": command, "environment": overrides,
              "native_assembly": environment.get("AWS_LC_SYS_PREBUILT_NASM"),
              "linker": environment.get("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"),
              "compiler_wrappers": "disabled",
              "build_directory": environment["CARGO_BUILD_BUILD_DIR"],
              "rustc": subprocess.check_output(["rustc", "-Vv"], text=True, env=environment).strip(),
              "runs": []}
    for scenario in ("fresh", "unchanged", "binary_edit", "library_edit"):
        if scenario.endswith("_edit"):
            edited = source / "src" / ("main.rs" if scenario == "binary_edit" else "config.rs")
            with edited.open("a", encoding="utf-8") as stream:
                stream.write(f"\n// Build benchmark: {scenario}.\n")
        print(f"Starting {scenario}: {' '.join(command)}", flush=True)
        memory_samples = []
        stop_sampling = threading.Event()

        def sample_memory():
            while not stop_sampling.is_set():
                available = available_memory_gib()
                if available is not None:
                    memory_samples.append(available)
                stop_sampling.wait(0.5)

        sampler = threading.Thread(target=sample_memory, daemon=True)
        sampler.start()
        started = time.perf_counter()
        try:
            with (output / f"{scenario}.jsonl").open("w", encoding="utf-8") as stdout, \
                    (output / f"{scenario}.log").open("w", encoding="utf-8") as stderr:
                result = subprocess.run(command, cwd=source, env=environment, stdout=stdout, stderr=stderr)
            elapsed = time.perf_counter() - started
        finally:
            stop_sampling.set()
            sampler.join()
        artifacts = [json.loads(line) for line in (output / f"{scenario}.jsonl").read_text(encoding="utf-8").splitlines()
                     if line.startswith("{")]
        rebuilt = [item["target"]["name"] for item in artifacts
                   if item.get("reason") == "compiler-artifact" and not item["fresh"]]
        measurement = {"scenario": scenario, "seconds": round(elapsed, 3),
                       "exit_code": result.returncode, "rebuilt": rebuilt}
        if memory_samples:
            measurement["minimum_free_memory_gib"] = round(min(memory_samples), 2)
        report["runs"].append(measurement)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({**measurement, "rebuilt": len(rebuilt)}), flush=True)
        if result.returncode:
            raise SystemExit(f"Build failed; see {output / (scenario + '.log')}")
        if scenario == "unchanged" and rebuilt:
            raise SystemExit("Unchanged build unexpectedly recompiled targets")
    if args.command == "build":
        executable = next(item["executable"] for item in artifacts
                          if item.get("reason") == "compiler-artifact" and item.get("executable"))
        subprocess.run([executable, "--help"], check=True, stdout=subprocess.DEVNULL)
    print(f"Report: {output / 'report.json'}", flush=True)


if __name__ == "__main__":
    main()
