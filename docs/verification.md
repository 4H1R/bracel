# Bracel extraction verification

Executed 2026-09-26 on WSL Ubuntu, Rust 1.98.1 and PostgreSQL 18.6:

- Workspace cargo check passed.
- scripts/check.sh passed: formatting, Clippy with warnings denied, 22 tests, OpenAPI drift, cargo-deny and optimized builds.
- The packaged library passed a consumer test in a temporary project outside this workspace, with application-defined state and no database configuration. The CLI package compiled independently.
- scripts/container-smoke.sh passed: migrations, readiness, anonymous/bearer routes, query filters, rate/CORS behavior, persistence, log redaction, non-root/read-only runtime and SIGTERM.

The first moved-test run exposed an altered JSON newline escape; it was corrected before the full successful run. A temporary Windows shell wrapper initially had CRLF line endings; correcting it allowed the container checks to execute. Neither failure was skipped.

The two GitHub repositories were created private under 4H1R. No crates.io package or CI secrets were published. Standalone CI is manual until private-framework access is configured; the framework workspace CI covers the reference starter automatically. Prior capability evidence is retained in the starter's verification record.
