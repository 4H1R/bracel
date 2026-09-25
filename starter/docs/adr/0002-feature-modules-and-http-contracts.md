# Feature modules with shared HTTP contracts

Accepted 2026-09-25. Keep one Axum + SeaORM crate organized into feature modules: a feature owns its DTOs, application operations, private persistence model and route module. Shared HTTP code owns response envelopes, Problem Details, pagination encoding/limits and middleware; application assembly connects these modules. This preserves locality for humans and AI while avoiding mandatory repository interfaces or domain layers for simple CRUD. Add domain logic when business invariants warrant it, and integration interfaces where behavior actually varies.

Successful JSON resources use `Data<T>` and collections use `Page<T>`; errors retain RFC 9457 Problem Details. This intentionally changes the teaching API's previous bare success objects, including health responses. OpenAPI and runtime tests define the wire contract. Bodyless responses, files and streams retain their native semantics.

Use forward keyset pagination over immutable creation time and unique ID, with bounded pages and versioned collection-specific cursors. This avoids large offsets and keeps navigation predictable under newer inserts, but does not provide a frozen multi-request snapshot or arbitrary page-number jumps. The new migration preserves existing rows and records a migration-time backfill because historical creation timestamps were never stored.

See [architecture](../architecture.md), [HTTP](../http.md), and [data](../features/data.md) for the implementation and extension rules.
