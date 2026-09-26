# Native Windows development

Keep the checkout on the Windows drive. Install Rust through Windows Rustup with
the `x86_64-pc-windows-msvc` host, Visual Studio 2022 Build Tools with the Desktop
development with C++ workload (including the Windows SDK and CMake), Git for
Windows, and Python 3.11+ with the `py` launcher. Rustup selects the version in
`rust-toolchain.toml` and installs its requested components.

Open a new PowerShell terminal after installing Rust so it receives the updated
PATH. From the framework checkout:

```powershell
rustc -vV
cargo --version
.\scripts\windows.ps1 cargo build --workspace --locked
.\scripts\windows.ps1 scripts/dev.sh up
.\scripts\windows.ps1 scripts/dev.sh migrate
.\scripts\windows.ps1 scripts/dev.sh run
```

Create `starter/.env` from `starter/.env.example` before running the development
commands, and set its database connection for your local PostgreSQL instance.
`dev.sh up` starts the Compose services on their configured ports.

`windows.ps1` explicitly selects Git for Windows Bash, adds Windows Cargo to the
process PATH, and supplies the scripts' `python3` command through `py -3`. This
avoids Windows' legacy `bash.exe` launcher, which starts WSL, and the Microsoft
Store `python3` shortcut. It uses `target/native-windows` unless you supply
`CARGO_TARGET_DIR`, keeping these builds separate from other builds. The launcher
restores the caller's environment and working directory when finished.
It defaults to two compiler jobs to limit simultaneous MSVC linking; set
`CARGO_BUILD_JOBS` explicitly to override that limit.

Quote Cargo's argument separator when calling through PowerShell, for example:
`.\scripts\windows.ps1 cargo clippy --workspace --all-targets --all-features '--' -D warnings`.

For Rust tests, use a disposable PostgreSQL database:

```powershell
$env:TEST_DATABASE_URL = 'postgres://starter:starter@127.0.0.1:57540/starter'
.\scripts\windows.ps1 cargo test --workspace --locked --all-targets --all-features
```

The example port must match your disposable database. Do not point the test suite
at a database containing application data.

Rust builds and the application run as Windows processes. Docker Desktop may
still use its WSL backend for PostgreSQL and Linux container checks. Native
Windows PostgreSQL can be used instead for development; building the deployment
image still requires a Linux container engine. Linux CI and the container checks
remain separate from native Windows verification.

## Local verification — 2026-09-27

Windows x64 was checked with Rust/Cargo 1.98.1, Visual Studio 2022 Build Tools
17.14.37710.0, Windows SDK 10.0.26100.0, Git Bash, and native Python. PostgreSQL
18.6 ran in a dedicated disposable Docker container on port 57540.

Passed:

- `cargo build --workspace --all-features --locked`.
- `cargo test --workspace --locked --all-targets --all-features --jobs 2`:
  45 tests passed across 26 test executables.
- Formatting and Clippy across all targets/features with warnings denied.
- `cargo deny --locked --all-features check`: advisories, bans, licenses and
  sources passed (duplicate dependency versions remain warnings).
- The five existing Python starter-export tests.
- All 11 AI companion fixture scenario groups under native Python and Rust.
- A real Windows server process: migrations twice, database inspection,
  readiness/health, registration, authenticated profile, logout, and rejection
  of the revoked token. Nine assertions passed.
- Both PowerShell entry points, Git Bash/Python selection, literal compiler
  arguments, exit-code propagation, and restoration of the caller's environment.

The first unrestricted parallel test build reported MSVC `LNK1104` for installed
runtime libraries. The two-job retry passed; the precise transient cause was not
established. The launcher therefore defaults to two jobs.

The complete native shell gate is not claimed as passed. Its AI fixture failure
on CRLF files was fixed and those fixtures then passed; its final workspace
metadata check reported a stale `.bracel/ai.lock.json` while shared changes were
being finalized. Linux distribution/container gates are verified separately.

Local logs and the repeatable HTTP smoke harness are in
`.scratch/native-windows/`; the HTTP report records the executable SHA-256. The
AI fixture report is `target/native-windows/ai-e2e.json`. The disposable database
and HTTP server were removed/stopped after verification.
