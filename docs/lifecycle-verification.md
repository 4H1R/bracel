# Lifecycle verification — 2026-09-27

This records observed checks for the CLI lifecycle implementation. The release
workflow has not been dispatched, and no versions, tags or public releases were changed.

## Focused acceptance

- Windows CLI compilation, formatting and Clippy with warnings denied passed.
- A complete renamed starter ran native Windows diagnostics through the PowerShell
  launcher and compiled all source/test targets with its locked Git dependencies.
  Evidence: `target/native-windows/lifecycle-native.json` and the adjacent log.
- `scripts/lifecycle-e2e.py` passed seven scenario groups on Windows and Linux:
  missing prerequisites; complete project naming; Docker orchestration; native
  commands and environment parsing; upgrade previews/application preservation;
  failed-upgrade restoration; checksum-verified replacement of the running CLI.
  Both platforms also exercised latest-release discovery through the isolated transport.
  Upgrades aligned an older pinned Rust toolchain; a failed upgrade restored the
  original manifest, lockfile and toolchain file byte for byte.
- `scripts/installer-e2e.py` passed installation into a directory with spaces,
  corrupt-checksum rejection, and wrong-version rejection on Windows. Existing
  executables were preserved on failure. All three cases also passed on Linux.
- `scripts/lifecycle-container.py` passed a real fresh project with PostgreSQL
  18.6 and Mailpit: build, migration/readiness, registration/profile, password-reset
  delivery, Docker AI context setup/inspection, and database preservation after
  stop/restart. The CLI had no dependency on host Cargo for this path.
- `bash scripts/container-smoke.sh` passed the deployment-image checks, including
  migrations, diagnostics, readiness, authentication, filters, rate limits/CORS,
  persistence, redacted logs, non-root/read-only runtime and SIGTERM.
- ShellCheck passed `install.sh` and `scripts/lifecycle-smoke.sh`. Workflow YAML
  parsed successfully. The canonical capability catalog and `git diff --check`
  passed.

## Repository gate

`bash scripts/check.sh` ran against a disposable PostgreSQL database on Linux.
Its log records successful formatting, Clippy, workspace tests, lifecycle and AI
fixtures, feature checks, dependency policy checks, package verification, an
independent consumer, generator smoke tests, all 13 API acceptance runs, and the
final account acceptance run. Evidence: `.scratch/lifecycle-check.log` and the
acceptance artifacts referenced there. The terminal session was interrupted after
completion, so its final process exit code was not retained. The focused lifecycle
checks were rerun afterward on Windows and Linux and both returned exit code 0.

Repeatable JSON artifacts live under `target/native-windows/` for the Windows
acceptance runs and under `target/` for Linux fixtures. The real Docker run also
retains `target/native-windows/lifecycle-container.log`. These are generated local
evidence, not committed binaries. The fixture tests use isolated Git repositories
and replacement download transports; the real Docker test uses the matching
published starter tag. Test-owned containers and volumes were removed afterward.

## Platform and release limits

The macOS installers/binaries are configured in the release matrix but have not
been executed on this machine. Prebuilt installer downloads require a coordinated
release with the binary assets and `SHA256SUMS`. Until then, the new CLI can be
built from this checkout. Application framework upgrades preserve application
source and migration history; compile checks do not replace application behavior
tests or reviewed production migrations.
