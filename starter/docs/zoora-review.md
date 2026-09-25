# Techniques adapted from Zoora

A sub-agent inspected the Go backend at WSL `/home/ghost/web/zoora` read-only on 2026-09-25, at revision `0a2970c1f23e70c02a8fd3b175e1d41998ab0f77`. Its working tree was clean. The inspection read source and repository guidance, not secrets, and did not run Zoora's tests. These are observations about that revision, not a claim of runtime verification.

| Observed technique | Source in the WSL checkout | Application to this starter |
| --- | --- | --- |
| Existing persistence operations can participate in a caller-owned transaction | `/home/ghost/web/zoora/internal/platform/database/tx.go:23,39`; `/home/ghost/web/zoora/internal/customfields/service.go:76` | Implemented using SeaORM's existing `ConnectionTrait` in `notes::create_note`, with PostgreSQL commit/rollback coverage. No new repository interface or context-hidden transaction. |
| Successful mutation and audit record commit together; denied actions have separate best-effort capture | `/home/ghost/web/zoora/docs/adr/0004-audit-log-synchronous-service-capture.md:3,10`; `/home/ghost/web/zoora/internal/audit/service.go:26,38` | Added optional [audit guidance](features/identity.md#durable-audit-history-when-required), including audit-failure rollback and target-tenant attribution. No audit tables or runtime code added. |
| Negative tests cover tenant-aware permission delegation and nested response privacy | `/home/ghost/web/zoora/internal/domain/security_boundaries_test.go:36,64` | Added acceptance cases to the [identity recipe](features/identity.md). Authentication remains recipe-only. |

The transaction adaptation is deliberately Rust-native: callers pass either a pool or transaction explicitly, and own commit/rollback. The new test proves a note is invisible outside its transaction before commit, visible afterward, and absent after a later conflicting write triggers rollback. The [jobs recipe](features/jobs.md) now composes note creation with intent insertion using that same operation.

Zoora also has worker correlation and deduplicated failure alerts in `/home/ghost/web/zoora/internal/platform/queue/server.go:49` and `/home/ghost/web/zoora/internal/platform/queue/monitor.go:23,34`. The jobs cookbook already addresses worker diagnosis and failure handling. Its Redis workers, domain-specific interfaces and broad service/repository layering serve a larger application; they do not justify extra dependencies or layers in this core. The starter retains its existing generated request IDs and safe readiness errors.

For executed starter checks, see [verification](verification.md). Source references here are comparison evidence; implementing recipes must not depend on having Zoora checked out.
