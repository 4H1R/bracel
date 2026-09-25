# Verification record

## Bracel 0.2 batteries (2026-09-26)

Executed in WSL2 Ubuntu with Rust 1.98.1 and disposable PostgreSQL 18.6. `bash scripts/check.sh` passed: three Python export tests, strict Clippy, 35 workspace Rust tests with all features, independent compilation of each integration feature and the framework without optional features, OpenAPI drift, cargo-deny, release builds, package verification and an independent packaged-framework consumer. The generator smoke test renamed the application crate, previewed without writes, rejected conflicts/path traversal, then generated and compiled CRUD and ran all 23 application tests. It checked persisted updates, cross-owner reads/writes/cursors, validation, unique OpenAPI operation IDs and problem media types.

`bash scripts/container-smoke.sh` passed the image build, all migrations, diagnostics, readiness, filtering, bearer auth, limits/CORS, persistence, redacted logs, non-root/read-only runtime and SIGTERM. `git diff --check` and the Markdown link scan passed (169 links before this evidence update).

New integration evidence: in-memory mail capture and a loopback SMTP protocol fixture exercised acceptance and permanent rejection; connection refusal and HTML escaping passed. Local HTTP fixtures exercised origin restrictions, redirects, response bounds and deadlines. Memory/filesystem storage exercised scope separation, missing objects and size bounds. Cache tests exercised scoped keys, coalesced loads, invalidation, TTL and oversized/failed loads. A loopback OTLP HTTP collector received traces; an unavailable collector did not prevent bounded shutdown. No real recipients, cloud credentials or hosted providers were used. S3 configuration compiled but a live S3 provider was not exercised; provider-specific deployment, browser login, calendar cron, shared cache/quota infrastructure and automatic metrics/context propagation remain outside this release.

Resolved during implementation: a generated filter test expected 422 instead of the established 400; asynchronous authentication initially held a non-Sync request borrow; an OpenAPI JSON construction error and strict-lint findings were corrected. The first dependency-policy run rejected Lettre's quoted_printable 0.5.2 under 0BSD; the policy now allows that exact package/version after checking the [license](https://opensource.org/license/0bsd). One package-check run failed because its script was edited while the shell was reading it; the stable final full check passed. These failures were corrected, not skipped. Existing transitive duplicate-version warnings remain allowed by policy.

Bracel 0.1.1 cleanup verified on 2026-09-26: `bash scripts/check.sh` passed in the framework workspace with the disposable PostgreSQL 18.6 database: 22 Rust tests, two export validation tests, formatting, strict Clippy, OpenAPI drift, dependency policy, release build and an independent packaged-library consumer test. `bash scripts/container-smoke.sh` passed migrations, diagnostics, readiness, filters, bearer auth, limits/CORS, persistence, redaction, restricted runtime and shutdown. The cleanup shares timestamp bounds between filters and cursors, makes application HTTP configuration explicit, removes unused direct dependencies, and derives release tags/export versions from manifests. README branding is local repository artwork; GitHub social-preview settings were not changed.

Bracel distribution verified on 2026-09-26: the reference application passed in the framework workspace (22 tests including framework tests). A standalone export pinned to framework commit 541ba3131ccf828e234e5898ea83f35d4949d535 fetched its dependency from GitHub and passed its complete check script (16 application tests) and container smoke. The standalone Docker build used vendored locked sources and the same authentication/filtering/rate-limit/readiness/shutdown scenarios. Standalone CI remains manual until access to the private framework dependency is configured; no CI credentials were provisioned.

Date: 2026-09-25 (Asia/Dubai). Executed locally in WSL2 Ubuntu x86_64 with Docker Desktop Linux containers. Rust 1.98.1, Axum 0.8.9, SeaORM/migration 2.0.3, utoipa 6.0.0, cargo-deny 0.20.2. The application dependency graph is locked in `Cargo.lock`; `cargo tree -e features` confirmed no active SQLite/MySQL driver, Redis, SMTP or cloud client in the core. PostgreSQL image: `postgres:18.6-bookworm`, server reported 18.6. PostgreSQL's [18.6 release notes](https://www.postgresql.org/docs/release/18.6/) were checked before finalizing the image; 18.5 was not released.

## Core commands and observed results

### Shared queries and middleware (2026-09-25)

Implemented reusable SeaORM filter/sort declarations, bounded URL decoding, canonical filter/sort/principal cursor binding, named route access policies, issuer-key JWT verification, local rate/concurrency limits, CORS and extracted request context. Notes is the complete HTTP example. OpenAPI and inspect consume the same query declarations, with explicit route scope/rate metadata. No new database migration was needed.

| Command actually executed | Observed result |
| --- | --- |
| `cargo fetch`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings` | Resolved and compiled jsonwebtoken 11.1.0 (aws-lc-rs), Governor 0.10.4, tower-http 0.7.1 (CORS) and direct sha2 0.10.9. Initial compile errors during extraction/schema work were corrected. Final strict Clippy passed. |
| `bash scripts/dev.sh up` and `bash scripts/dev.sh migrate` | Local PostgreSQL 18.6 ready and both existing migrations current. First migrate attempt lacked DATABASE_URL; reran with explicit local configuration successfully. |
| `bash scripts/openapi.sh write` | Generated filters, sort enum, bearer security and middleware response contracts. The first quality run caught nondeterministic extension ordering; the generator now serializes through an ordered JSON object map. Final drift checks passed. |
| `bash scripts/check.sh` with TEST_DATABASE_URL set | Passed in full on the final graph: formatting, warnings-denied Clippy, all 21 tests, OpenAPI drift, cargo-deny advisory/license/source policy, and locked release build. Duplicate-version warnings remain allowed by repository policy. |
| `bash scripts/container-smoke.sh` | Passed with disabled examples, anonymous examples and bearer-protected examples, filter/sort HTTP requests, 401 without a token, 200 with valid access tokens, 429/Retry-After with CORS, log redaction, migrations, diagnostics, read-only non-root runtime and clean SIGTERM. One rerun hit the PostgreSQL temporary-initialization-socket race; changed the script to wait for TCP readiness and the final rerun passed. |
| `bash scripts/dev.sh run` with BIND_ADDR=127.0.0.1:3107 | Live health HTTP request returned 200/data.status=ok, a generated request ID and nosniff header; disabled notes returned 404. Ctrl-C drained and exited 0. |
| `cargo tree --locked -e features` | Final Governor uses only std, JWT uses aws-lc-rs/PEM and tower-http uses CORS. Unused Governor jitter, dashmap and quanta features were removed; full checks and container smoke passed after that change. No SQLite/MySQL driver or Redis client was introduced. |
| `git diff --check` and local Markdown-link scan | Passed; 122 relative file links resolved before adding this evidence section. |

New coverage includes literal wildcard/backslash/SQL-shaped filters, exact UUID/text and date bounds, ascending traversal, equivalent timestamps, changed filter/sort/principal rejection, mandatory base predicate preservation, forged signatures, wrong algorithms/type/issuer/audience, missing claims, expired/future tokens, duplicate Authorization, read/write scope enforcement, authenticated writes, CORS preflight/error headers, independent principal quotas, ignored forwarded IP headers, bounded limiter storage/cleanup, concurrency rejection and timeout permit release. Existing migrations, transaction composition, validation/error contracts, diagnostics and pagination tests still pass.

Authentication is verified locally with public test-only RSA fixtures; no external issuer/login flow was exercised. AUTH_MODE defaults off for teaching; configuring bearer requires a real issuer public key, issuer and API audience. Notes scopes govern a shared collection, not per-user ownership/tenant isolation. Quotas are process-local, key rotation requires configuration/restart, and CSRF remains part of the unimplemented cookie-session recipe. These limitations are recorded in [middleware](middleware.md), [identity](features/identity.md) and [data](features/data.md).

### Feature layout, shared responses and cursor pagination (2026-09-25)

Moved notes into `src/features/notes/`, shared HTTP contracts/middleware into `src/http/`, and the ordered migration history into `src/migrations/`. Extracted the shared PostgreSQL/HTTP test fixture to `tests/support/`. Added `Data<T>` and `Page<T>`, a timestamp/ID cursor list endpoint, a non-destructive timestamp/index migration, an architecture guide and ADR. Success JSON, including probes, now has a `data` envelope; error bodies retain the prior Problem Details contract. The original migration name and SQL are preserved.

Commands ran in WSL2 Ubuntu using `CARGO_TARGET_DIR=/tmp/bracel-starter-target`, the Windows Docker CLI through `DOCKER_BIN`, and the loopback starter database. Docker was initially stopped; starting Docker Desktop succeeded on this run, unlike the earlier tooling run below.

| Command actually executed | Observed result |
| --- | --- |
| `cargo check --lib --bin bracel-starter` | First attempt found utoipa did not recognize SeaORM's timestamp type alias; added an explicit string/date-time schema annotation. The subsequent compile passed. |
| `cargo check --locked --all-targets` | Passed for the final feature/test structure. |
| `bash scripts/openapi.sh write` | Regenerated shared success schemas, the note timestamp, and list query/response contracts. |
| `bash scripts/dev.sh up` and `bash scripts/dev.sh migrate` | Passed against local PostgreSQL 18.6. Both migrations are applied. |
| `bash scripts/check.sh` with `TEST_DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter` | Passed in full after final code changes: format, Clippy with warnings denied, 14 tests (4 unit + 3 CLI + 4 HTTP/database + 3 pagination), OpenAPI drift, cargo-deny and locked release build. Existing duplicate-version warnings remain. |
| `bash scripts/container-smoke.sh` | Passed: image build, migrations twice, doctor/inspect, both example modes, shared envelopes, list response, invalid limit, persistence, redaction, restricted runtime and clean SIGTERM. |
| `bash scripts/dev.sh run` with local database and `ENABLE_EXAMPLE=true` | Started successfully; live HTTP checks verified `data.status` and notes `data`/`page`. Ctrl-C drained and exited with code 0. |
| PostgreSQL `EXPLAIN (COSTS OFF)` on a temporary 10,000-row table cloned with the notes indexes | Planner selected `Index Scan using note_pagination_plan_created_at_id_idx` with the tuple predicate as `Index Cond` and `LIMIT 26`. Fixture transaction rolled back. This is query-plan evidence, not a latency benchmark. |
| `cargo tree -e features` | Reviewed timestamp support and base64; no active SQLite/MySQL driver introduced. |
| `git diff --check` and Markdown relative-path checks | Passed. |

Dependency changes reuse locked versions: base64 0.22.1 is now a direct dependency for bounded URL-safe cursor encoding, and SeaORM's `with-chrono` feature supports PostgreSQL timestamps. The lockfile only adds base64 to the application dependency list; no package versions changed. No identity, queue, cloud or other optional runtime was added.

Pagination scenarios cover empty/default/max/exact-final pages, tied microsecond timestamps, mixed UUID v4/v7 IDs, deletion of the anchor, insertion ahead of the cursor, disabled routes, malformed/oversized/version/scope/position errors, duplicate/unsupported query parameters and validation before database access. A populated first-migration schema upgrades without losing records; timestamp/index down/up preserves the note; readiness fails while the timestamp migration is missing. Contract tests verify OpenAPI envelope references and required page fields. Existing transaction composition and error-header/redaction tests still pass.

This PostgreSQL 18/container run clears the earlier developer-tooling verification blocker. Remaining limits are documented in [data](features/data.md): teaching routes remain unauthenticated, cursors are unsigned navigation positions, traversal is not a frozen snapshot, and old rows use migration-time backfill. The optional email exercise was not rerun.

### Developer tooling follow-up (2026-09-25)

Added `doctor [--json] [--deploy] [--database]` and `inspect [--json] [--database]`, strict CLI parsing, selected locked package metadata, opt-in read-only migration history and documented JSON/exit contracts. No dependencies, schema changes or OpenAPI changes. Container and disposable email-copy builds now include `build.rs`; container smoke also invokes both diagnostic commands. The following evidence applies to this change and does not inherit the earlier container pass below.

| Command actually executed | Result for this change |
| --- | --- |
| `cargo check --locked --all-targets` | Passed in WSL2 Ubuntu with Rust 1.98.1. |
| `cargo fmt --all` and `cargo clippy --locked --all-targets -- -D warnings` | Passed. |
| `cargo test --locked --lib --test cli` | Passed: 4 unit tests and 3 subprocess CLI tests. |
| `bash scripts/dev.sh up` | Blocked: Docker Desktop Linux engine pipe unavailable. |
| `bash scripts/dev.sh migrate` | Initial attempt failed because no `.env`/`DATABASE_URL` was supplied; the subsequent sequence with the explicit local URL stopped at Docker startup. No migration of the intended Docker database occurred. |
| `bash scripts/check.sh` with the standard port-5432 `TEST_DATABASE_URL` | Failed: all four database scenarios could not connect. CLI/unit tests passed; the script stopped before later stages. |
| `bash scripts/openapi.sh check` | Passed; contract unchanged. |
| `cargo deny --locked check` | Passed advisories, bans, licenses and sources; existing duplicate-version warnings. |
| `cargo build --locked --release --bin bracel-starter` | Passed. |
| `bash scripts/check.sh` with `TEST_DATABASE_URL=postgres://ghost@127.0.0.1:55439/postgres` | Passed in full on a disposable PostgreSQL **16.15** fallback: 4 unit + 3 CLI + 4 HTTP/database scenario tests, format, Clippy, OpenAPI drift, cargo-deny and release build. |
| `bash scripts/container-smoke.sh` with the Windows Docker CLI via `DOCKER_BIN` | Blocked before build: Docker engine pipe unavailable. New container diagnostic assertions have not executed. |
| `git diff --check` | Passed. |

Fallback setup: discovered PostgreSQL client tools but no server; downloaded Ubuntu's `postgresql-16` package with `apt-get download`, extracted it into a generated `/tmp/starter-tooling-pg.*` directory using `dpkg-deb -x` (no system installation), initialized a UTF-8 disposable cluster, bound it to loopback port 55439 and stopped it after checks. An initial inline harness had shell quoting trouble and a subsequent attempt found the missing server before this extraction succeeded. Used `CARGO_TARGET_DIR=/tmp/bracel-starter-target`. Temporary artifacts are outside tracked source.

The new database scenario verifies that a fresh schema remains empty after inspection, pending history fails doctor but remains inspectable, applied history passes, both route inventories match the actual router, locks time out and unknown history fails without leaking its database-supplied sentinel. CLI tests verify one clean JSON document, exit codes, invalid arguments/config, example/deploy gating, offline behavior with an unreachable database and secret redaction.

Docker Desktop startup failed on `sailor-ingest.sock` in its local runtime directory. A narrow removal attempt failed because Windows could not access that socket; no factory reset or Docker data deletion was performed. The supported PostgreSQL 18.6/container checks remain unverified for this change; the PostgreSQL 16 fallback does not expand the supported matrix. The optional email exercise was not rerun.

### Earlier baseline

The following commands were executed from the repository with `CARGO_TARGET_DIR=/tmp/bracel-starter-target` and `TEST_DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter`. This test database contains local-only credentials; tests isolate data in random schemas.

| Command | Observed result |
| --- | --- |
| `docker compose up -d --wait` | PostgreSQL healthy; final image 18.6 |
| `cargo check --all-targets` | Passed during initial implementation |
| `cargo fmt --all -- --check` | Passed via shared check script |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked --all-targets` | Passed: 4 unit tests + 3 HTTP/PostgreSQL scenario tests; no ignored tests |
| `bash scripts/openapi.sh write` then `bash scripts/openapi.sh check` | Generated checked-in spec; drift check passed |
| `cargo deny --locked check` | Advisories, bans, licenses, sources passed; transitive duplicate-version warnings remain intentionally visible |
| `cargo build --locked --release --bin bracel-starter` | Passed |
| `bash scripts/check.sh` | Passed as one complete workflow against PostgreSQL 18.6 |
| `bash scripts/container-smoke.sh` | Passed: Docker build, repeated migrations, readiness, both example settings, create/get persistence, sensitive-log sentinel checks, read-only/non-root runtime and clean SIGTERM |

HTTP/database scenarios cover schema not migrated, migration up twice, create/read and direct row inspection, trimming/Unicode validation, invalid JSON/types/unknown fields/content type, oversized body, malformed/missing UUID, 404/405 with Allow, problem media type/status/request ID, representative OpenAPI required fields, database lock deadline, closed pool, health independent of database, and migration down/up. Both enabled and disabled example modes are exercised. Each scenario asserts multiple behaviors; the test function count is not a count of all assertions.

The host Docker CLI is Windows-only in this setup, so Docker scripts used `DOCKER_BIN="/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe"` in WSL. Normal Linux or Docker Desktop with WSL integration uses the scripts' default `docker`. Initial Docker access needed local sandbox escalation, and Docker Desktop was started. No production environment or remote messaging service was contacted.

## Optional recipe trial

`bash scripts/exercise-email.sh` created a disposable application copy under `.scratch/`, added Lettre 0.11.23 and Askama 0.16.1 there, and used Mailpit 1.31.2 on loopback. The core `Cargo.toml`, `Cargo.lock`, `src/`, and startup configuration remain free of these integrations. The script was rerun after correcting an escaping assertion: Askama emits numeric HTML entities.

Passed in the copy: Clippy for all trial targets, both email tests (HTML escaping/Unicode, invalid recipient, SMTP capture and connection refused), and a Mailpit API assertion for exactly one message with expected recipient, subject, plain text and escaped HTML. No external account or real recipient was used. The capture container was removed on script exit; ignored copies remain for review. The local trial did not exercise a production SMTP TLS provider, provider feedback, durable delivery, or an email-enabled Docker image.

The email recipe records the additional template Docker context/copy steps needed on adoption. Authentication, jobs, scheduling, uploads, caching, metrics, realtime, backup/restore and provider integrations remain documentation-only guidance; they were not compiled or executed merely because email succeeded.

## Corrections and remaining limits

Zod-style validation update on 2026-09-25: replaced the `errors` map with ordered `issues`, each containing a typed code, a path of string fields/integer indexes, and a safe message. HTTP cases assert the new codes, root paths and absence of the old map; a new normalization test verifies nested/indexed paths, literal dotted/numeric keys, multiple issues for one field and merge order. Regenerated OpenAPI with the code enum and path union. All seven tests and the complete `scripts/check.sh` and `scripts/container-smoke.sh` passed. An initial test assumed `anyOf`; it was corrected to the generated `oneOf` schema before the successful run. No dependencies or database migrations changed.

Field-validation update on 2026-09-25: added Laravel-style field-to-message arrays to HTTP 422 problem responses and a reusable `ValidationErrors` collector. HTTP/PostgreSQL checks cover missing/null/blank/wrong-type/overlong title, unknown fields together with a title error, duplicate title, non-object input, redaction of submitted data, and no inserts for rejected requests. Non-validation failures omit `errors`. Regenerated OpenAPI and verified its field-message schema. All six tests and the full `scripts/check.sh` and `scripts/container-smoke.sh` passed; no dependencies or database migrations were added.

UUID v7 update on 2026-09-25: switched note IDs, request IDs and generated test fixtures to `Uuid::now_v7()`. Existing HTTP/PostgreSQL scenarios now assert version 7 for generated note and request IDs and verify a stored legacy v4 note remains readable. Both `scripts/check.sh` (all six tests, OpenAPI drift, dependency policy and release build) and `scripts/container-smoke.sh` passed. `cargo tree -e features -i uuid` confirmed v7 enabled with no v4 feature; `Cargo.lock`, database schema and generated OpenAPI were unchanged.

Zoora-informed extension on 2026-09-25: a read-only sub-agent comparison identified transaction composition, durable audit semantics and negative authorization/serialization tests; [the comparison record](zoora-review.md) contains source evidence and scope. `notes::create_note` now accepts SeaORM's `ConnectionTrait`. A new real-PostgreSQL test verified uncommitted isolation, commit visibility, and rollback after a conflicting second write. The full `scripts/check.sh` and `scripts/container-smoke.sh` passed again. All 20 local Markdown documents' file links resolved. Audit and authorization changes are recipe guidance only; Zoora's tests were not run and its files were not changed.

Refactor verification on 2026-09-25: repeated `scripts/check.sh` and `scripts/container-smoke.sh` successfully after replacing error-body JSON reparsing with typed `AppError` response metadata and one problem-details renderer. Added a routed regression test for header preservation (including CORS/cookies), safe fallback details, stale body-header removal, correlation IDs and unchanged success responses. OpenAPI output remained identical. Also executed the OpenAPI write path with a fake generator that emitted partial output and exited 42: the command failed and the original specification hash remained unchanged. Redundant comments were removed; dependencies and migrations were unchanged.

The first cargo-deny run rejected the Mozilla root certificate data license. The policy now explicitly allows `CDLA-Permissive-2.0` and passes. One base-image download ended with unexpected EOF; a retry succeeded. Initial tests used PostgreSQL 18.3, then the final image was updated to 18.6 and the full checks were repeated. These failures are resolved, not skipped checks.

No remaining local core verification blocker is known. GitHub CI is provided but has not run on a remote host; repository hosting/branch protection and review settings are instructions only. Native Windows and ARM are not tested. Public access control, backups, deployments, provider credentials and production hardening are future-project work. Container tags and advisory databases may change; run the scripts again when adopting or updating the starter.
