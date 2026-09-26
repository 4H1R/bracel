# Optional API packages

Implementation record, 2026-09-26. These packages are included in the 0.3.0 release.

| Package | Owns | Dependency direction |
| --- | --- | --- |
| bracel | HTTP lifecycle, JSON contracts, validation, queries, policies and bearer primitives | Axum, SeaORM; compatibility re-export of optional jobs |
| bracel-jobs | PostgreSQL workers, typed jobs, delayed/named queues and schedules | No dependency on core; explicit application migrations |
| bracel-data | Idempotent transactions, version checks, tenants/audit, search and deletion conventions | Core/PostgreSQL |
| bracel-realtime | Transactional events, ordered retained publication, SSE and optional sockets | Core/PostgreSQL; no broker required |
| bracel-delivery | Durable notifications and signed incoming/outgoing webhooks | Core, jobs, realtime and optional provider adapters |
| bracel-files | Authorized upload/attachment lifecycle and reconciliation | Core, storage adapter and PostgreSQL |
| bracel-integrations | Mail, storage, cache, outbound HTTP, provider discovery and telemetry adapters | Optional dependencies only |

The starter owns routes, state, domain records and migration history. Each package supplies explicit schema installation/upgrade SQL; constructing a handle never migrates. Existing public paths and released SQL remain compatible. Transport code stays API-only. Administrative operations use authenticated JSON or trusted CLI commands.

Local [user accounts](../starter/docs/accounts.md) are a starter feature: users, profile policy and reset intent migrations stay with the application. They reuse core bearer/token primitives and the mail adapter. The starter enables mail by default; this does not add password or SMTP dependencies to Bracel core.

## Failure scenarios specified before implementation

Tests cross real HTTP/socket/CLI interfaces with PostgreSQL and local protocol peers. Each completed E2E run emits a machine-readable report, redacted transcript, commands, source fingerprint and service versions under the ignored .scratch directory. No new unit tests follow implementation.

| Workflow | Failure behavior to verify |
| --- | --- |
| Validation/PATCH | Unknown/duplicate fields, omitted versus null, nested array paths, invalid UUID/date/decimal/enum/range, invalid cross-field input; no partial writes |
| Queries | Invalid/unallowed filters/includes/fields, bounded lists, both cursor directions, changed access/filter context, count bounds, private relation fields |
| Mutations | Same key/same request replay, changed request conflict, concurrent duplicates, rollback/retry, expired keys; stale version rejected |
| Tenant/audit/search | Nonmember and other-owner denial, audit and write rollback together, deleted records hidden, restore conflicts, authorized search only |
| Events/realtime | Uncommitted intent invisible, concurrent commit ordering, duplicates, lost wake-up, two replicas, reconnect/retention gap, expired/revoked credentials, slow reader, connection limits, shutdown |
| Notifications/webhooks | Duplicate intent, disabled channel, transient/permanent provider failure, bad signature/stale timestamp, replay, unauthorized inbox, bounded delivery and retention |
| Files | Forged owner/key, excess bytes, checksum/type mismatch, duplicate completion, missing object, partial failure, expiry and cleanup, deletion/download race |
| Jobs/schedules | Unknown payload/version, delayed/named queue isolation, lease loss, bounded parallelism, schedule overlap and DST/missed runs, replay and shutdown |
| Provider identity | Issuer/audience/type mismatch, unknown key, rotated key, outage/stale cache, bounded response; no token-provided URL discovery |
| Operations | Untrusted forwarded headers, stream-safe middleware, bounded metrics labels, context propagation, exporter outage and secret redaction |

## Progress

- [x] Public-process E2E harness and initial failing scenario
- [x] Core validation/query/mutation conventions
- [x] Optional package ownership and backward compatibility
- [x] Durable events, SSE and WebSockets
- [x] Notifications, files and webhooks
- [x] Typed jobs/schedules and provider identity
- [x] Tenant/audit/search and production diagnostics
- [x] Starter/CLI integration, generated contracts and accurate inspection
- [x] Full checks, container checks, package consumption and dated evidence

The [implementation guide](../starter/docs/api-packages.md) records actual interfaces and boundaries. The failure table is a verification target, not a claim that every case has already passed. See the dated acceptance evidence before making deployment claims.

Final verification on 2026-09-26: the workspace check, eight packaged crates, independent consumer, generated application and optional-feature container smoke passed. Thirteen real-process scenarios made 102 assertions. The local evidence manifest is `.scratch/api-packages-evidence.json`; it records matching source/binary hashes across scenarios, archive hashes, report/log paths and the tested container image. [Verification details](../starter/docs/verification.md#optional-api-packages-2026-09-26).
