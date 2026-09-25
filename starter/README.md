# Bracel starter

A Bracel application on Axum + SeaORM + PostgreSQL, with shared query filters, cursor pagination and [route middleware](docs/middleware.md). Bearer JWT verification, local rate limits, CORS and concurrency limits are included. PostgreSQL is the only required external runtime service; Redis, browser login, email, jobs and cloud services remain recipes in the [capability cookbook](docs/features/index.md).

The framework is a dependency; this repository owns features, migrations, state and deployment. In the framework workspace it uses a local path; standalone releases pin an exact Git revision. During private development, authenticate Git for access to 4H1R/bracel before building. Standalone CI requires separately configured access to that private dependency; credentials are not provisioned here. Build the standalone image with `bash scripts/build-image.sh`: it vendors locked sources outside Docker and builds without network access or credentials in image layers. No crates.io publication is implied.

## Run locally

Prerequisites: Rust via rustup (the repository installs/selects 1.98.1), a C compiler/linker, Bash, diff, OpenSSL CLI for container-test tokens, and Docker with Compose. Linux is the supported build/deployment platform. On Windows use WSL2 Ubuntu with Rust/build-essential installed and Docker Desktop's WSL integration enabled. Native Windows builds are not in CI. The email exercise additionally uses Python 3 and curl.

```bash
cp .env.example .env
bash scripts/dev.sh up
bash scripts/dev.sh migrate
bash scripts/dev.sh run
```

The example environment deliberately opts into unauthenticated example routes on loopback. Without `ENABLE_EXAMPLE=true`, those routes return 404. Keep them off on a public deployment until you implement access control. `.env` is sourced as shell configuration by the development script; the binary itself only reads process environment. Only source trusted local files.

In another terminal:

```bash
curl -i http://127.0.0.1:3000/healthz
curl -i http://127.0.0.1:3000/readyz
curl -i -H 'Content-Type: application/json' \
  -d '{"title":"My first note"}' http://127.0.0.1:3000/example/notes
# Copy the returned data.id:
curl http://127.0.0.1:3000/example/notes/REPLACE_WITH_ID
curl --globoff 'http://127.0.0.1:3000/example/notes?filter[title]=note&sort=-created_at&limit=25'
# Pass page.next_cursor unchanged as ?limit=25&after=... for the next page.
```

`healthz` checks the process; `readyz` queries the migrated database. Ctrl-C drains in-flight requests. `bash scripts/dev.sh down` stops dependencies and retains the database volume.

## Check your changes

Inspect configuration and application structure without changing the database:

```bash
bash scripts/dev.sh doctor
bash scripts/dev.sh inspect --json
bash scripts/dev.sh doctor --json --database
```

Database checks are opt-in. `doctor --deploy --database` also rejects enabled teaching routes; run it with the intended deployment environment. [Developer tooling](docs/features/tooling.md) documents JSON output, exit codes and limits.

```bash
cargo install cargo-deny --version 0.20.2 --locked
export TEST_DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter
bash scripts/check.sh
bash scripts/container-smoke.sh
```

Database tests create random isolated schemas and remove them after success; use a disposable database. Checks fail if the database is absent. CI runs these same scripts. [Quality policy](docs/quality.md) explains the supported configurations and required hosting settings. [Verification record](docs/verification.md) distinguishes observed results from instructions.

## Make this your project

1. Copy or use this repository as a template. Keep `Cargo.lock`, `rust-toolchain.toml`, and `STARTER_VERSION`. Initialize a new Git repository if needed. Change the package name in `Cargo.toml`; update the Rust crate imports in `src/main.rs`, `src/bin/openapi.rs`, and `tests/http.rs`, plus binary/image names in scripts and Dockerfile. Run `cargo check` to refresh the lockfile package entry, then run the checks.
2. Follow [architecture](docs/architecture.md): replace `src/features/notes/`, its route registration in `src/http/mod.rs`, tests, and OpenAPI annotations with your first feature. For a fresh, unused database, replace the teaching migrations. For a database already in use, append migrations instead of editing history. Also change the readiness query to the schema your application requires. Remove `ENABLE_EXAMPLE` once the teaching routes are gone.
3. Read `AGENTS.md`, the [catalog](docs/features/index.md), and the relevant recipe. Ask, for example: **“Add durable email notifications for newly created notes using `docs/features/email.md` and `docs/features/jobs.md`. Store the note and delivery intent atomically, test retries and duplicate delivery, and update the catalog with actual results.”**
4. Record the source URL and source commit in `STARTER_VERSION` when adopting the starter. Tag your initial import. Keep an optional `starter` Git remote and review future upstream diffs; cherry-pick reviewed changes rather than overwriting your application. Re-run all checks after dependency, migration, or recipe updates.

The [architecture decision](docs/adr/0001-stack.md) explains why this uses Axum rather than Loco. [HTTP](docs/http.md), [database](docs/database.md), and [operations](docs/operations.md) describe the core. Configure [bearer authentication](docs/features/identity.md#implemented-bearer-access-tokens) for API clients. The notes example has shared read/write scopes, not per-user ownership or tenant isolation, and no hosted deployment is configured.
