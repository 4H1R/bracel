# Framework verification

## Coordinated framework and starter changes — 2026-09-27

The complete `CARGO_INCREMENTAL=0 bash scripts/check.sh` passed in WSL Ubuntu with Rust 1.98.1 and disposable PostgreSQL 18.6. This includes the AI companion checks, formatting, strict Clippy, workspace/database tests, independent feature configurations, API contract drift, dependency policy, release builds, all package archives, the independent consumer, generated application and all real-process API/account scenarios. The full-feature production container smoke also passed.

The final exporter correction additionally passed seven Python regressions on native Windows (including `py -X utf8=0`) and Linux: UTF-8 input and LF generated files preserve export hashes across platforms. The regressions first reproduced CRLF output and a Windows encoding failure.

Detailed commands, artifacts and limitations are in the [starter verification record](../starter/docs/verification.md), [native Windows evidence](windows.md), and [AI companion evidence](ai-verification.md). No package versions or release tags were changed. Remote publication is a separate step.

## Framework extraction — 2026-09-26

Executed 2026-09-26 on WSL Ubuntu, Rust 1.98.1 and PostgreSQL 18.6:

- Workspace cargo check passed.
- scripts/check.sh passed: formatting, Clippy with warnings denied, 22 tests, OpenAPI drift, cargo-deny and optimized builds.
- The packaged library passed a consumer test in a temporary project outside this workspace, with application-defined state and no database configuration. The CLI package compiled independently.
- scripts/container-smoke.sh passed: migrations, readiness, anonymous/bearer routes, query filters, rate/CORS behavior, persistence, log redaction, non-root/read-only runtime and SIGTERM.

The first moved-test run exposed an altered JSON newline escape; it was corrected before the full successful run. A temporary Windows shell wrapper initially had CRLF line endings; correcting it allowed the container checks to execute. Neither failure was skipped.

The two GitHub repositories were created private under 4H1R. No crates.io package or CI secrets were published. Standalone CI is manual until private-framework access is configured; the framework workspace CI covers the reference starter automatically. Prior capability evidence is retained in the starter's verification record.

The standalone export fetched framework revision 541ba3131ccf828e234e5898ea83f35d4949d535 from GitHub and passed its complete check script, including all 16 application tests. CLI help/version and refusal to overwrite an existing directory also passed.

Development wrapper checks passed for Compose startup, idempotent migration and JSON framework-version inventory. Passing the Compose project explicitly fixed Docker Desktop's environment forwarding behavior; the temporary conflicting project's unused resources were removed and the original database was retained. LF checkout rules preserve shell-script compatibility on Windows.

The standalone container smoke also passed after vendoring all locked dependencies outside Docker. An installed CLI cloned the starter's v0.1.0 tag, created a main branch, and produced an application passing cargo check --locked and offline JSON inspection. CLI Clippy passed after this final generation change. The initial framework GitHub container job passed; its core job was still running when local verification finished.
