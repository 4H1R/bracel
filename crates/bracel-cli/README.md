# Bracel CLI

Install from the framework workspace with cargo install --path crates/bracel-cli.
Run bracel new my-api to clone the matching versioned starter into a new directory.

Requires Git and access to https://github.com/4H1R/bracel-starter.
Existing destinations are never overwritten. The starter remote and its shallow
history remain available for reviewing future template changes.


Inside an application, run bracel make resource Project --field name:string --crud.
Use --dry-run --json to inspect the plan without writing files. Required string,
i64 and bool fields are supported. Generated code includes owner-scoped CRUD,
validation, pagination, migration registration and PostgreSQL HTTP tests.

## AI companion

Run `bracel ai install --agents codex,claude,cursor` in an application with fetched,
locked dependencies. `bracel ai sync` refreshes generated context; `bracel ai sync --check`
detects drift. `bracel ai search "query"` reads version-matched source and docs, and
`bracel ai mcp` exposes local project, search, capability, inspection and diagnostic tools.
See [the companion guide](../../docs/ai.md) for overrides, bundles, configuration and validation.
