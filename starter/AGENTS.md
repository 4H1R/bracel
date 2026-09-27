# Working in this starter

Use Bracel with Axum + SeaORM + PostgreSQL. Application features and migrations stay in this crate; shared HTTP/query/auth code belongs to the framework dependency. Read [architecture](docs/architecture.md) and [the stack decision](../docs/adr/starter-0001-stack.md) before changing module organization. The teaching slice is `src/features/notes/`, `tests/http.rs` and `tests/pagination.rs`.

- For HTTP routes, validation, errors, limits, or OpenAPI, read [HTTP conventions](../docs/guides/http.md).
- For entities, migrations, transactions, or database tests, read [database conventions](docs/database.md).
- For authentication, authorization, sessions, or tenants, read [identity](../docs/guides/features/identity.md).
- For jobs, retries, events, or webhooks, read [background work](../docs/guides/features/jobs.md); for timers, also read [scheduling](../docs/guides/features/scheduling.md).
- For environment variables, deployment, shutdown, secrets, or recovery, read [operations](docs/operations.md).
- For dependencies, toolchain changes, CI, or review policy, read [quality](docs/quality.md).

Start with the [README prerequisites](README.md). Use `bash scripts/dev.sh up`, `bash scripts/dev.sh migrate`, and `bash scripts/dev.sh run`. Run `bash scripts/check.sh` with `TEST_DATABASE_URL` set, then `bash scripts/container-smoke.sh`. Script implementations are authoritative.

When adding a capability: read [the framework catalog](../docs/guides/features/index.md) and its recipe; follow the notes slice; include failure-path tests; update OpenAPI and application docs with actual behavior and dated evidence. Keep optional dependencies out until requested. Shared framework guides are linked from [the README](README.md#framework-guides); standalone links match the dependency revision in `STARTER_VERSION`. Record application-specific rules locally.

Report commands actually executed and their results. Distinguish failed or blocked checks from passing checks. Recipe guidance is not evidence that an integration works.
