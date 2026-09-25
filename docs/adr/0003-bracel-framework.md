# Bracel library and separately consumable starter

Accepted 2026-09-26. Bracel replaces the single-application distribution with a library and CLI workspace; the application stays a modular monolith, consuming the library's public interfaces. Keeping the reference starter in this workspace allows coordinated contract tests, while exporting a revision-pinned standalone starter gives adopters their own application and migration history without copying framework implementation.

This supersedes the single-crate distribution constraint in [the original stack decision](../../starter/docs/adr/0001-stack.md). Axum, SeaORM, PostgreSQL, explicit feature modules and selective DDD remain unchanged.
