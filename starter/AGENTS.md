# Working in this starter

Use Bracel with Axum + SeaORM + PostgreSQL. Application features and migrations stay in this crate; shared HTTP/query/auth code belongs to the framework dependency. Read [architecture](docs/architecture.md) and [the stack decision](docs/adr/0001-stack.md) before changing module organization. The teaching slice is `src/features/notes/`, `tests/http.rs` and `tests/pagination.rs`.

- For HTTP routes, validation, errors, limits, or OpenAPI, read [HTTP conventions](docs/http.md).
- For entities, migrations, transactions, or database tests, read [database conventions](docs/database.md).
- For authentication, authorization, sessions, or tenants, read [identity](docs/features/identity.md).
- For jobs, retries, events, or webhooks, read [background work](docs/features/jobs.md); for timers, also read [scheduling](docs/features/scheduling.md).
- For environment variables, deployment, shutdown, secrets, or recovery, read [operations](docs/operations.md).
- For dependencies, toolchain changes, CI, or review policy, read [quality](docs/quality.md).

Start with the [README prerequisites](README.md). Use `bash scripts/dev.sh up`, `bash scripts/dev.sh migrate`, and `bash scripts/dev.sh run`. Run `bash scripts/check.sh` with `TEST_DATABASE_URL` set, then `bash scripts/container-smoke.sh`. Script implementations are authoritative.

When adding a capability: read [the catalog](docs/features/index.md) and its recipe; follow the notes slice; implement the smallest complete behavior, including failure-path tests; update OpenAPI if applicable; update the recipe/catalog with the actual implementation and dated evidence. Keep optional dependencies out until requested.

Report commands actually executed and their results. Distinguish failed or blocked checks from passing checks. Recipe guidance is not evidence that an integration works.
