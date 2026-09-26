# Working on Bracel

Read [the framework interface](docs/framework.md) and [the split decision](docs/adr/0003-bracel-framework.md) before changing crate ownership. Bracel keeps Axum, SeaORM and PostgreSQL; applications own state, migrations and features.

- Framework HTTP, auth, queries and middleware live in crates/bracel/. Test through public interfaces with application-defined state.
- Starter changes follow [starter instructions](starter/AGENTS.md) and the capability recipe they link.
- CLI and release changes follow [distribution](docs/distribution.md).

Run bash scripts/check.sh with a disposable TEST_DATABASE_URL, then bash scripts/container-smoke.sh. The check script also packages Bracel and runs a consumer outside the workspace. Scripts are authoritative; report actual results and distinguish blocked checks.

Do not introduce domain-specific configuration, entities, migration history or route paths into the library. Keep the starter's reference source here; export it with the release script instead of maintaining a second hand-edited copy.

## Testing

- NEVER write unit tests after you write code.
- Highly prefer E2E tests as the sole testing mechanism. Use them to verify complex features work. At the end of E2E tests, produce a verifiable and repeatable artifact.
- If you must test a system in isolation, FIRST write all the ways it could fail, THEN write the code.

## Publishing

Keep changes local. Do not push, bump versions, or create release tags or releases until the user explicitly asks.
