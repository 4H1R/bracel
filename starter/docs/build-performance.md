# Build performance

## Choose the build you need

From the application directory on Windows:

```powershell
# Fast type checking while editing.
.\scripts\windows.ps1 cargo check --locked

# Runnable development build and tests, with line-number backtraces.
.\scripts\windows.ps1 cargo build --locked --bin bracel-starter
.\scripts\windows.ps1 cargo test --locked --all-targets

# Full debugger variable/type information when needed.
.\scripts\windows.ps1 cargo build --locked --profile dev-full --bin bracel-starter

# Optimized local iteration with incremental compilation.
.\scripts\windows.ps1 cargo build --locked --profile release-fast --bin bracel-starter

# Shipping optimization settings.
.\scripts\windows.ps1 cargo build --locked --release --bin bracel-starter
```

On Linux, run the corresponding `cargo` command directly. Database tests require
`TEST_DATABASE_URL` pointing to a disposable PostgreSQL database.

The default development/test profile uses `debug = "line-tables-only"`. This
retains filename/line backtraces but omits local-variable/type debugging.
`dev-full` restores full debug information. `release-fast` inherits release with
optimization level 2, local LTO only, and incremental compilation. It is intended
for local iteration; application runtime performance has not been benchmarked
against release. The shipping profile retains optimization level 3 and ThinLTO.
[Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).

## Windows defaults

The launcher selects at most sixteen compiler jobs with at least 12 GiB free RAM,
eight with at least 8 GiB, four with at least 4 GiB, and two otherwise, capped by logical CPU count. Failed
memory detection uses the two-job fallback. Set `CARGO_BUILD_JOBS` to override.

For the native x64 MSVC toolchain, it uses the pinned Rust installation's bundled
`rust-lld.exe`. It preserves explicit linkers, compiler flags, custom compilers,
toolchains, targets and relevant Cargo configuration. Set `BRACEL_LINKER=msvc`
to retain MSVC's linker. LLVM describes its Windows compatibility in the
[LLD documentation](https://lld.llvm.org/windows_support.html).

When NASM is absent, the launcher enables the pinned AWS-LC dependency's bundled
Windows assembly objects. Explicit `AWS_LC_SYS_PREBUILT_NASM` settings are
preserved. The launcher restores the caller's environment when it exits.

Keep the target directory, profile, features, linker and compiler flags stable
between edits. Changing them can require rebuilding dependencies. The launcher
uses `target/native-windows` unless `CARGO_TARGET_DIR` is set.

## Measurements

Measured 2026-09-27 on Windows x64, Ryzen 9 7950X, 32 GB RAM, Rust 1.98.1,
default starter features. Baseline is four jobs/MSVC/full development debug
information. Tuned is eight jobs/bundled LLD/line-table development debug
information. Shipping release optimization settings are identical.

| Scenario | Baseline, 4 jobs | Tuned, 8 jobs | Tuned, 16 jobs |
| --- | ---: | ---: | ---: |
| Fresh development executable | 83.500 s | 49.928 s | 39.949 s |
| Development rebuild after binary edit | 4.563 s | 3.305 s | 3.416 s |
| Development rebuild after library edit | 4.492 s | 3.567 s | 3.356 s |
| Fresh `cargo check` | 58.678 s | 37.716 s | Not measured |
| Check after library edit | 1.024 s | 1.054 s | Not measured |
| Fresh test compilation | 96.595 s | 54.554 s | Not measured |
| Test recompilation after binary edit | 10.652 s | 5.401 s | Not measured |
| Test recompilation after library edit | 10.217 s | 5.909 s | Not measured |
| Fresh shipping release | 143.829 s | 85.455 s | 54.426 s |

With the tuned settings, shipping release rebuilds took 32.430 seconds after a
binary edit and 32.163 seconds after a library edit. The optional `release-fast`
settings at eight jobs took 5.228 and 3.731 seconds respectively. Its clean build was 85.590
seconds, essentially equal to shipping release in this run. It improves edited
builds, not the first build. At sixteen jobs, shipping release rebuilds took
23.290 and 24.838 seconds. The sixteen-job development and release builds retained
at least 11.2 and 11.35 GiB of free system RAM respectively.

Most unchanged commands took approximately 0.5 seconds. The tuned test-compilation
run took 7.048 seconds without recompiling any target; this outlier is retained in
the results rather than counted as an improvement.

The earlier two-job clean release took 262.735 seconds. A separate build-script
fix reduced an unchanged release build from 108.095 to 0.439 seconds by avoiding
watches for nonexistent files. Cargo documents this invalidation behavior in its
[FAQ](https://doc.rust-lang.org/stable/cargo/faq.html).

These are single runs, not averages or guarantees. Compilation was sequential
with no other project compilation scheduled by this task. Each clean run used
an empty target directory and disabled compiler wrappers. Downloaded registry
and Git sources were already available; network downloads, tool installation,
and launcher setup are excluded. Edited scenarios append a Rust comment to the
binary or library source in a copy; the application's embedded source provenance
changes, so these are real application rebuilds but not a model for every edit.

## Reproduce and inspect

The benchmark works with a standalone starter. It copies inputs, never modifies
the original source, and refuses an existing output directory:

```powershell
.\scripts\windows.ps1 py -3 scripts/benchmark-build.py --project . --output target/bench-dev --jobs 8
.\scripts\windows.ps1 py -3 scripts/benchmark-build.py --project . --output target/bench-release --jobs 8 --profile release
.\scripts\windows.ps1 py -3 scripts/benchmark-build.py --project . --output target/bench-check --jobs 8 --command check
.\scripts\windows.ps1 py -3 scripts/benchmark-build.py --project . --output target/bench-tests --jobs 8 --command test
```

Each directory contains `report.json`, Cargo JSON output, stderr logs and Cargo
HTML timing reports. Runs cover fresh, unchanged, binary-edit and library-edit
scenarios. The unchanged run fails if Cargo recompiles a target; executable
builds also run `--help`. `--env NAME=VALUE` supports controlled profile/linker
experiments. For an ordinary cache regression check without a fresh snapshot:

```powershell
.\scripts\windows.ps1 py -3 scripts/check-build-cache.py --release
```

The [recorded results](build-performance-results.json) retain every experiment,
including the failed linker-driver trial and the unchanged-test timing outlier.
The harness now explicitly disables configured compiler wrappers and pins its
intermediate build directory inside the fresh output, including when callers
have custom Cargo settings. Earlier runs were checked to have no such wrappers
or external build-directory settings. Temporary compiler caches were removed
after measurement; source snapshots, logs and timing reports were retained.

## Verification

On 2026-09-27, all 27 standalone native tests passed with all features enabled.
The real `dev`, `release-fast` and `release` binaries were built in the normal
output directory and their help commands passed. Both optimized binaries passed
live migrations, readiness, registration, authenticated profile, logout and token
revocation checks against disposable PostgreSQL (12 assertions total).

Workspace and standalone container smoke tests passed their canonical database,
HTTP, authentication, shutdown and restricted-runtime assertions. A workspace
build-layer rerun compiled only the four local crates, reusing all registry
dependencies; its Cargo step took 18.24 seconds. The standalone frozen/vendored
workflow passed without changing the lockfile. These validation runs overlapped
other checks and are not controlled timing comparisons.

The broader workspace checks passed formatting, Clippy with warnings denied,
all-feature tests, architecture variants, isolated optional-feature compilation,
OpenAPI drift, dependency policy, the full release build, all eight package
verifications and an independent packaged consumer. Native Windows execution
needed temporary test-harness path adaptations and a PostgreSQL client adapter
to the disposable database container.

Eleven of thirteen API scenarios passed. The realtime and middleware scenarios
failed graceful-shutdown assertions because Python's `Popen.terminate()` uses
Windows `TerminateProcess`: it cannot provide the graceful SSE close or telemetry
flush those assertions expect. Assertions were retained; these two native test
failures are not reported as passes. Linux container shutdown checks passed.
Account acceptance passed all 79 assertions. Generated-application formatting,
Clippy and 31 tests passed; its OpenAPI contract assertions passed in a resumed
tail run using explicit Git Bash and Windows-compatible path encoding. The
disposable PostgreSQL container was removed after database-dependent verification
completed.

Profile-export regression tests, the benchmark cache-isolation regression and
launcher checks passed. Launcher checks ran on PowerShell 7 and Windows
PowerShell 5.1 and include explicit overrides, failed-command cleanup, Git Bash
utilities and preserving nested shell PATH additions. Remote GitHub cache
hit/miss behavior has not been exercised.

## CI and containers

CI saves Cargo sources and development/release outputs by toolchain, manifests,
lockfile and commit, with dependency-compatible fallback restores. The pinned
`cargo-deny` executable has a separate cache. Real remote hit/miss behavior and
cache-transfer overhead still require a GitHub Actions run.

Docker uses architecture-specific BuildKit target caches and copies the finished
executable outside the cache mount within the build step. Local source inputs
are touched when the layer reruns, preventing stale application artifacts when
an older source snapshot is copied onto newer cached objects. Standalone image
builds retain frozen, vendored compilation. These mounts help on the same builder;
they do not automatically persist between fresh GitHub runners.
[Docker cache mounts](https://docs.docker.com/build/cache/optimize/#use-cache-mounts).

## Deliberate limits

The native AWS-LC build and final release optimization were the main costs.
Consolidating TLS providers could alter cryptography behavior and the dependency
critical path for little measured benefit, so the providers and application
features remain intact. No runtime security checks were removed. Compiler caches
such as sccache require reusable compiled results and do not make a genuinely
empty-cache build cheaper. No antivirus exclusions or machine-wide compiler
settings were applied.
