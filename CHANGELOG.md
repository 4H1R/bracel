# Changelog

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

For local accounts, select `AUTH_MODE=local`. New users receive only `account:self`; application resource permissions remain explicit. Run `auth:mail-work` with local Mailpit or configured TLS SMTP for password resets. Local sessions expire after one day; reset revokes existing sessions. Email verification, MFA and refresh-token flows are not included. See the [account guide](starter/docs/accounts.md) and [package guide](starter/docs/api-packages.md).
