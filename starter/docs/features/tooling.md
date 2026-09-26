# Developer diagnostics and inspection

Inspection uses `http::Registrations::configured` and validated provider settings, including SMTP relay configuration. `middleware.selected` reports the configured selection; `compression_active` also checks the Cargo feature and compression setting. The older `middleware.enabled` field remains an alias for selection for compatibility. Inspection does not construct providers or start background tasks.

Implementation update (2026-09-26): consult [the optional API package guide](../api-packages.md) for the current implemented workflows, package boundaries and limits. The design recipes below remain guidance for application-specific extensions; they are not verification evidence.

Implemented 2026-09-25 in `src/cli/tooling.rs` and the existing application binary. No new dependencies, database tables, HTTP endpoints or MCP server. Use these commands to inspect the application's configured behavior before an AI or human edits it.

```bash
bash scripts/dev.sh doctor
bash scripts/dev.sh doctor --json
bash scripts/dev.sh doctor --deploy --database --json
bash scripts/dev.sh inspect --json
bash scripts/dev.sh inspect --database --json
```

The development wrapper sources a trusted `.env` if present, like `run` and `migrate`. For deployment checks invoke the built binary directly with deployment environment variables: `bracel-starter doctor --deploy --database --json`. The binary never sources files. Help does not require configuration. Other commands validate the same `Config` as serving, including a required `DATABASE_URL`; an offline check parses the URL but does not connect. Invalid configuration identifies the first failing setting without printing its value. No command invokes migrations or fixes configuration automatically.

## Output and exit codes

`doctor` prints readable checks; `--json` emits one JSON document. `inspect` always emits JSON (`--json` is accepted explicitly). Neither diagnostic command initializes application logging. JSON has `schema_version: 1`, `command`, `ok`, `checks`, `database_status` and `migrations`. Inspection adds `application`. Each check includes stable `code`, `severity` (`info`, `warning`, `error`), `message` and `remediation`; consume codes rather than parsing prose. Consumers should tolerate additional fields/checks.

Exit 0 means the requested checks succeeded; warnings do not fail. Exit 1 means invalid configuration or a diagnostic error. Exit 2 means invalid arguments, with usage on stderr and no JSON; duplicate or unsupported options and trailing arguments are rejected without echoing them. `serve` remains the default command. `--deploy` applies only to `doctor`.

| Code | Meaning |
| --- | --- |
| `CONFIG.VALID` / `CONFIG.INVALID` | Shared configuration validation passed / failed. |
| `CONFIG.EXAMPLE_ENABLED` | Teaching routes are enabled; warning for local use. |
| `DEPLOY.EXAMPLE_ENABLED` | Teaching routes enabled in deployment mode; error. |
| `DEPLOY.EXTERNAL_REVIEW` | Proxy/TLS, privileges, secrets, capacity and recovery remain external checks. |
| `DATABASE.NOT_CHECKED` | No database connection was requested. |
| `DATABASE.SKIPPED` | Invalid configuration prevented a requested database check. |
| `DATABASE.UNAVAILABLE` | Connection failed or timed out. |
| `DATABASE.INSPECTION_FAILED` | History read failed, timed out, or is incompatible with this binary. |
| `DATABASE.PENDING_MIGRATIONS` | Pending history entries: error for doctor, warning for inspect. |
| `DATABASE.MIGRATIONS_CURRENT` | Migration history matches this binary; not proof of schema correctness. |

Without `--database`, `database_status` and every migration's status are `not_checked`, never presumed current. On successful inspection the database status is `checked` and migrations are `pending` or `applied`; failed database access reports `unavailable`. Inspection can successfully describe pending migrations, whereas doctor treats them as an actionable failure.

## Read-only database checks

`--database` connects with a single-connection pool, a five-second connection budget and a separate five-second inspection budget. Statements have a two-second PostgreSQL timeout; pool shutdown has a two-second budget. It uses a repeatable-read, read-only transaction and SeaORM's `get_migration_with_status_read_only`, which does not install migration history on an empty database. The selected database role needs visibility of the intended schema and SELECT on existing migration history; DDL rights are unnecessary. The configured PostgreSQL search path determines the inspected schema. Do not run this concurrently with deployment migrations and expect a final post-deployment result.

Unknown migration history or unreadable history fails safely. Raw database errors, connection URLs, credentials and unknown database-supplied migration names are not printed. Known migration names come from the compiled application. This checks migration history, not arbitrary schema drift, row integrity, notes-table readiness or upgrade safety. It does not run prior-release upgrade tests or provide a migration dry run.

## Application inventory

Query/middleware extension, 2026-09-25: each route now includes runtime authentication (`public`/`bearer`, null for invalid configuration), required scope, rate policy, and generated query parameters. The middleware inventory exposes numeric local quotas/capacities and whether CORS is configured. It never exposes tokens, PEM material, issuer configuration values or database credentials. Locked versions now include jsonwebtoken, governor and tower-http. Identity status is bearer_jwt; browser sessions and tenants remain recipes. New fields are additive to CLI schema_version 1.

Inspection includes the package name/version, minimum Rust version (not the executing compiler), selected core package versions extracted from `Cargo.lock` by `build.rs`, supported commands, selected capability statuses and local documentation paths. The versions belong to the compiled binary; running an old binary after editing source still describes that old build. Multiple locked versions of a selected package are listed rather than guessed. Documentation paths refer to the source checkout and need not exist inside the runtime container.

Routes come from generated OpenAPI, with the teaching routes' `ENABLE_EXAMPLE` gate applied. `enabled` is null when configuration is invalid; true means configured, not network reachable or healthy. Only documented explicit operations are enumerated; Axum's implicit HEAD handling and fallbacks are excluded. Runtime tests compare the inventory to actual route availability in both example modes. When adding routes/capabilities, update their annotations, gating and inventory together. Optional capabilities remain recipe-only until implemented; the catalog is the full capability list.

## Verification

Acceptance coverage lives in `tests/cli.rs` and `tests/http.rs`: clean JSON and exit codes; secret redaction; both route modes; offline operation with an unreachable URL; fresh-schema inspection without creating tables; pending/applied history; locked and incompatible migration history. Run `bash scripts/check.sh` with `TEST_DATABASE_URL`, then `bash scripts/container-smoke.sh`. See [the dated verification record](../verification.md) for commands actually executed and outcomes.

Observed 2026-09-25: the complete check script passed on a disposable PostgreSQL 16.15 fallback (all 11 tests, Clippy, OpenAPI drift, cargo-deny and release build). Docker Desktop failed startup on an inaccessible runtime socket, so the PostgreSQL 18 check attempt failed during database setup and container smoke failed before building. PostgreSQL 16 evidence does not replace the supported PostgreSQL 18/container gate. No supported-platform change is implied.

Later verification on 2026-09-25, during the feature-layout/pagination change: Docker Desktop started successfully. The full check script passed on PostgreSQL 18.6 (14 tests) and container smoke passed, including both CLI commands. This clears the earlier tooling verification blocker. Inventory now includes five explicit HTTP operations and both migrations, plus pagination/success-envelope capability flags and an architecture documentation link.
