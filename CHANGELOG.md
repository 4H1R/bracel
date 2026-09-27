# Changelog

## Unreleased — project lifecycle

- Added installers and an opt-in workflow for verified prebuilt CLI releases on Windows, Linux and macOS.
- Added prerequisite checks, complete project naming/configuration, Docker and native development commands, and Docker-backed AI companion commands.
- Added framework upgrade previews, exact revision updates, toolchain alignment and restoration after failed checks. CLI self updates are separate from application dependencies.
- Added lifecycle, installer and real Docker acceptance harnesses. See [getting started](docs/getting-started.md) for the workflow and release availability.

## 0.4.0 — 2026-09-27

Bracel adds application customization helpers, email verification and a revision-aware AI companion. Build defaults now reduce clean and edited compilation time. Framework packages, CLI and starter share this version.

- Added dedicated configuration points for middleware, current-user extraction, cache helpers, jobs, schedules and provider settings. Split starter bootstrap, CLI and optional workflows into focused modules.
- Added queued email verification and configurable account behavior, with forward migrations and expanded account acceptance checks.
- Added CLI AI context generation, source-matched search, diagnostics, stdio MCP tools and portable knowledge bundles.
- Fixed repeated builds caused by watching nonexistent files. Added memory-aware Windows compiler parallelism, bundled LLD selection, line-table development debug information, `dev-full` and `release-fast` profiles.
- Preserved workspace profiles in exact-revision starter exports. Added reproducible build benchmarks, GitHub Actions caches and BuildKit compiler caches.

### Build measurements and verification

On the measured Windows machine, clean release compilation fell from 262.7 to 54.4 seconds and clean development compilation from 83.5 to 39.9 seconds. These are single runs with downloaded dependencies available; shipping release optimization settings are unchanged. See the [build guide](docs/history/starter-build-performance.md) for settings, tradeoffs and full results.

Workspace checks, package verification, an independent consumer, both container smoke suites, generated application tests and account acceptance passed during preparation. Eleven of thirteen native Windows API scenarios passed; two graceful-shutdown assertions fail because Python's Windows termination helper force-kills the server. Linux container shutdown checks passed. Remote CI results are reported separately on the release commit.

### Adoption

Use matching `v0.4.0` framework/CLI and starter tags. Existing applications should review helper/configuration changes and apply the new forward migrations; preserve their own features and migration history. Default development builds retain line-number backtraces; use `--profile dev-full` for local-variable/type debugging. The optional `release-fast` profile trades shipping optimization for faster iteration.

Packages remain distributed through GitHub; this release does not publish to crates.io.

## 0.3.0 — 2026-09-26

Bracel adds API workflows as optional packages and includes a working local account lifecycle in the starter. The framework, CLI, all optional packages and starter share this version.

- Split durable jobs/schedules into `bracel-jobs`, with the existing core re-export retained. Added typed jobs, named/priority queues, bounded workers and timezone-aware calendar schedules.
- Added `bracel-data` for transactional idempotency, version preconditions, memberships and audit; `bracel-realtime` for retained events, SSE and WebSockets; `bracel-delivery` for notifications, queued mail and signed webhooks; and `bracel-files` for verified uploads and attachments.
- Expanded validation, PATCH, filtering, collection projection, bidirectional pagination, proxy handling, HTTP metrics/compression, configured-issuer discovery and tracing integrations.
- Expanded generators for resource fields, jobs, events, policies, commands and migrations, with dry-run output and conflict checks.
- Added starter users, Argon2id registration/login, profile read/update, revocable bearer sessions, logout/logout-all, database-backed account quotas and single-use password resets with a durable SMTP worker.
- Enabled mail in the starter's default features. Its example environment now selects local bearer accounts, disables teaching routes and starts Mailpit through Compose. Framework mail remains optional.
- Added real-process HTTP/PostgreSQL/SMTP/socket verification and repeatable redacted E2E artifacts. Enabled standalone starter CI for the public repository.

### Adoption

Use matching `v0.3.0` framework/CLI and starter tags. The standalone starter pins all framework dependencies to one exact source commit. Packages are available from GitHub; nothing is published to crates.io.

Applications retain ownership of users, state and migrations. Review starter changes before adopting them; do not overwrite application migration history. Run forward migrations before serving new routes. Once the optional package migration has been applied, keep its Cargo feature compiled and disable endpoints with configuration instead of removing migration history.

For local accounts, select `AUTH_MODE=local`. New users receive only `account:self`; application resource permissions remain explicit. Run `auth:mail-work` with local Mailpit or configured TLS SMTP for password resets. Local sessions expire after one day; reset revokes existing sessions. Email verification, MFA and refresh-token flows are not included. See the [account guide](starter/docs/accounts.md) and [package guide](docs/guides/api-packages.md).
