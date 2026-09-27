<p align="center">
  <img src="assets/brand/banner.png" alt="Bracel — Rust backend framework" width="760">
</p>

# Bracel

[Get started](docs/getting-started.md) · [Framework guide](docs/framework.md) · [AI companion](docs/ai.md) · [Starter repository](https://github.com/4H1R/bracel-starter) · [MIT license](LICENSE)

An opinionated Rust backend framework built on Axum, SeaORM and PostgreSQL. Bracel provides typed collection filters, cursor pagination, success/error contracts, bearer JWT verification, route scopes, rate limits, CORS, request IDs, deadlines and structured request logs.

This repository contains the framework library, the CLI, and a reference starter used to test them together. Applications own their features, database connections, migration history and readiness checks.

| Location | Purpose |
| --- | --- |
| crates/bracel/ | Reusable library; no notes schema, global application state or database startup |
| crates/bracel-cli/ | Project creation and editable CRUD resource generation |
| crates/bracel-jobs/ | Durable typed jobs, queues and schedules |
| crates/bracel-data/ | Idempotent transactions, versions, membership and audit |
| crates/bracel-realtime/ | Retained events, SSE and optional WebSockets |
| crates/bracel-delivery/ | Inbox, queued mail and signed webhooks |
| crates/bracel-files/ | Verified uploads, scoped attachments and cleanup |
| crates/bracel-integrations/ | Optional mail, storage, cache, HTTP, identity and telemetry adapters |
| starter/ | Reference application and source for the separate starter repository |
| scripts/ | Workspace checks, package consumer verification and starter export |

New applications use Docker development by default. The prebuilt CLI needs Git and Docker with Compose; the Rust compiler and database run in containers. See [installation, local development and upgrades](docs/getting-started.md) for installer availability and native development. To try the new workflow from this checkout before its release, install the CLI with `cargo install --path crates/bracel-cli --locked` (requires local Rust and a linker).

~~~bash
bracel setup
bracel new my-api
cd my-api
bracel dev
~~~

Both repositories are public and MIT-licensed. Git is required; no crates.io release has been published. The CLI creates a starter matching its own version and retains a shallow starter history and a starter remote; add your own origin before publishing an application. See [the changelog](CHANGELOG.md) for v0.4.0 and upgrade notes.

To develop this framework workspace, install Rust 1.98.1 via rustup, a C compiler/linker, Bash, diff, OpenSSL, Python 3.11+, and Docker with Compose. Native Windows uses the [PowerShell launcher](docs/windows.md). Then:

~~~bash
cp starter/.env.example starter/.env
bash scripts/dev.sh up
bash scripts/dev.sh migrate
bash scripts/dev.sh run
export TEST_DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter
bash scripts/check.sh
bash scripts/container-smoke.sh
~~~

Read [the framework interface](docs/framework.md), [the split decision](docs/adr/0003-bracel-framework.md), and the [capability catalog](docs/guides/features/index.md). Bracel 0.2 adds resource generation, unified route contracts, validated requests, test helpers, record policies, application commands, durable PostgreSQL jobs/scheduling and revocable machine tokens. The starter includes [API user accounts](starter/docs/accounts.md): registration, password login, profiles, logout and queued password resets. Mail, storage, cache, outbound HTTP and OTLP tracing are optional framework integrations; the starter enables mail by default. Start with [the batteries guide](docs/guides/batteries.md). Cookie-based browser login and distributed rate limiting remain application work.

The [optional API packages](docs/guides/api-packages.md) add transactional idempotency, richer validation/PATCH, collection navigation, durable events, SSE/WebSockets, notifications, webhooks, uploads, named queues, calendar schedules and configured-provider key discovery. Enable only the packages an application needs. Core stays API-only and applications own domain behavior. See [package boundaries and failure scenarios](docs/packages.md). Open ideas are collected in the [backlog](docs/backlog.md).
