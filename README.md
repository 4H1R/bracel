<p align="center">
  <img src="assets/brand/banner.png" alt="Bracel — Rust backend framework" width="760">
</p>

# Bracel

[Get started](starter/README.md) · [Framework guide](docs/framework.md) · [Starter repository](https://github.com/4H1R/bracel-starter) · [MIT license](LICENSE)

An opinionated Rust backend framework built on Axum, SeaORM and PostgreSQL. Bracel provides typed collection filters, cursor pagination, success/error contracts, bearer JWT verification, route scopes, rate limits, CORS, request IDs, deadlines and structured request logs.

This repository contains the framework library, the CLI, and a reference starter used to test them together. Applications own their features, database connections, migration history and readiness checks.

| Location | Purpose |
| --- | --- |
| crates/bracel/ | Reusable library; no notes schema, global application state or database startup |
| crates/bracel-cli/ | CLI to clone the versioned starter |
| starter/ | Reference application and source for the separate starter repository |
| scripts/ | Workspace checks, package consumer verification and starter export |

Prerequisites: Rust 1.98.1 via rustup, a C compiler/linker, Bash, diff, OpenSSL, Python 3.11+ for exports and checks, and Docker with Compose. Linux is supported; use WSL2 on Windows. See the [starter guide](starter/README.md) for development commands and configuration.

~~~bash
cargo install --path crates/bracel-cli --locked
bracel new my-api
~~~

Git and access to the private 4H1R/bracel-starter and 4H1R/bracel repositories are required during this initial development phase. No crates.io release has been published. The CLI retains a shallow starter history and a starter remote; add your own origin before publishing an application.

To develop this workspace:

~~~bash
cp starter/.env.example starter/.env
bash scripts/dev.sh up
bash scripts/dev.sh migrate
bash scripts/dev.sh run
export TEST_DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter
bash scripts/check.sh
bash scripts/container-smoke.sh
~~~

Read [the framework interface](docs/framework.md), [the split decision](docs/adr/0003-bracel-framework.md), and the [capability catalog](starter/docs/features/index.md). Bracel is an initial 0.1 framework: application-specific doctor/inspect and OpenAPI registration currently live in the starter. Browser login, token issuance, distributed rate limiting and background jobs remain application work or documented recipes.
