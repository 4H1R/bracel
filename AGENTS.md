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

<!-- bracel:begin -->
# Bracel project context

At task start call `project_info` (or `bracel ai info`). Search version-matched documentation with `search_docs` before unfamiliar API work. Read current source and tests before editing. Run `bracel ai doctor` to identify stale context. Read matching application rules in `.ai/rules/index.md`. Reload the agent session after updating instructions or skills.

Resolved framework dependencies:
- bracel 0.3.0 (local source; inspect working tree)
- bracel-integrations 0.3.0 (local source; inspect working tree)
- bracel-jobs 0.3.0 (local source; inspect working tree)

## Framework boundaries

Applications own domain features, state, configuration, migrations and process lifecycle.
Bracel supplies reusable HTTP, auth, query and optional workflow mechanics. Resolve
the application's selected dependencies before choosing an API; a globally installed
CLI does not identify the application's framework revision.

Use the capability catalog to distinguish implemented APIs, optional features and
recipes. Read the returned source/example references. Resolved dependencies, compiled
features, configured services and verified behavior are separate facts.

For feature changes, update the canonical documentation, capability entry and executable
example in the same change. Run the repository's authoritative checks and report actual
results. Keep application conventions in `.ai/rules` with a `paths` YAML list. Use
`.ai/guidelines` or `.ai/skills` for intentional overrides of generated guidance.

<!-- bracel:end -->
