# Native Windows development

Install Windows Rustup, Visual Studio 2022 Build Tools with the Desktop development
with C++ workload and Windows SDK, Git for Windows, and Python 3.11+ with the `py`
launcher. Keep the checkout on your Windows drive and open a new PowerShell
terminal after installing Rust. The repository selects Rust 1.98.1 automatically.

From the application directory:

```powershell
rustc -vV
cargo --version
.\scripts\windows.ps1 cargo build --locked
# Create .env only if it does not already exist, then review its settings.
if (-not (Test-Path .env)) { Copy-Item .env.example .env }
.\scripts\windows.ps1 scripts/dev.sh up
.\scripts\windows.ps1 scripts/dev.sh migrate
.\scripts\windows.ps1 scripts/dev.sh run
```

The launcher uses native Windows Cargo and Git Bash. It does not invoke Windows'
legacy `bash.exe` launcher, which starts WSL. It supplies `python3` through the
Windows Python launcher and places build outputs in `target/native-windows`
unless `CARGO_TARGET_DIR` is already set. It restores the caller's environment and
working directory afterward.
It defaults to two compiler jobs to limit simultaneous MSVC linking; set
`CARGO_BUILD_JOBS` explicitly to override that limit.

Quote Cargo's argument separator when calling through PowerShell, for example:
`.\scripts\windows.ps1 cargo clippy --all-targets --all-features '--' -D warnings`.

To run tests, set `TEST_DATABASE_URL` to a disposable PostgreSQL database and run
`.\scripts\windows.ps1 cargo test --locked --all-targets --all-features`.

Rust builds and the server run as Windows processes. Compose starts PostgreSQL
and Mailpit through Docker Desktop; Docker may still use WSL internally for those
Linux containers. You can also configure `.env` to connect to native Windows
PostgreSQL and an SMTP service. Deployment-image checks require a Linux container
engine. Linux CI and the full shell acceptance suite are separate from native
Windows build/runtime verification.

## Verification scope

On 2026-09-27, the reference framework workspace passed its native Windows
all-feature build, all 45 Rust tests, formatting, Clippy, dependency-policy
checks, and nine real-process HTTP/database assertions. The Windows application
handled migrations, health/readiness, registration, authenticated profile access,
logout, and token revocation. The full shell distribution suite and Linux
container checks have a separate verification scope; Windows is not yet a CI
target.
