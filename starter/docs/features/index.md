# Capability catalog

The current optional workflows and their explicit limits are documented in [API packages](../api-packages.md). Runtime evidence is recorded separately in [verification](../verification.md); compiled support is not cloud-provider verification.


Bracel 0.2 adds implemented framework batteries. Use [the batteries guide](../batteries.md) for current interfaces and limits.

| New framework capability | Implementation |
| --- | --- |
| Resource generation | Editable CRUD, migration, fixture and HTTP test; dry-run JSON and conflict checks |
| Unified route registration | Runtime router, OpenAPI, policies and inspection from one registration |
| Validated extractors | JSON/query/path decoding, safe aggregated field errors |
| Test toolkit | Isolated PostgreSQL schemas, bearer-aware client, assertions and fixture identifiers |
| Record policies | Application policy interface and owner-scoped reads and writes |
| Application commands | Discoverable metadata, shared context, custom handlers and JSON results |
| Durable jobs and intervals | PostgreSQL leases, retries/replay, transactional intent and coalescing schedules |
| Machine tokens and rotation | Hashed credentials with expiry/revocation; overlapping RSA key sets |
| Optional integrations | Mail, scoped storage, cache, outbound HTTP and OTLP traces |

Read the relevant recipe before adding dependencies. **Implemented** means present and exercised; **Recipe** means instructions exist, with the verification level below; **Decision required** means project constraints must select the approach first. Dates and results are in each recipe and [verification](../verification.md). Optional libraries are feature-gated and absent from the default build.

| Capability | Status / verification | Recommended approach | Prerequisites / recipe |
| --- | --- | --- | --- |
| Framework distribution | Implemented 2026-09-26; workspace, packaged consumer and standalone application verified | Bracel dependency; application owns features/state/migrations | [Architecture](../architecture.md), [verification](../verification.md) |
| Developer diagnostics and AI-readable inspection | Implemented; PostgreSQL 18 and container checks passed on 2026-09-25 | `doctor`, `inspect --json`, opt-in read-only migration checks | [Developer tooling](tooling.md) |
| HTTP, validation, JSON errors, OpenAPI | Implemented; issue aggregation, typed paths and runtime/contract tests | Axum, utoipa, problem details with Zod-style issues | [HTTP](../http.md) |
| PostgreSQL pool, versioned migrations, transactions | Implemented; pool/migrations/transaction composition tested | SeaORM concrete feature logic | [Database](../database.md), [data](data.md) |
| Logs, request IDs, health, readiness, shutdown, limits | Implemented; core and container checks | JSON tracing, explicit deadlines | [HTTP](../http.md), [operations](../operations.md) |
| Authentication, sessions/tokens, authorization | JWT verification, key rotation, machine tokens, route scopes and generated ownership implemented; browser sessions remain recipes | Configured RSA issuer key, audience and scope policies | [Identity](identity.md), [middleware](../middleware.md) |
| Local users, registration, login, profile and password reset | Implemented in the starter; real HTTP/PostgreSQL/SMTP acceptance | Argon2id, revocable bearer sessions, one-use reset credentials; email verification remains application work | [Accounts](../accounts.md) |
| Tenant isolation | Membership-checked context and shared tenant reference implemented | Memberships and tenant-scoped queries | Identity + tenant model; [identity](identity.md) |
| Durable audit history | Transactional success audit implemented | Commit success audit and mutation together; separately capture denials | Identity + retention policy; [identity](identity.md#durable-audit-history-when-required) |
| Durable jobs, retries, failed-job inspection/replay | Implemented in 0.2; PostgreSQL concurrency tests | PostgreSQL lease queue + separate worker | Delivery semantics; [jobs](jobs.md) |
| Events, outbox, webhooks | Retained transactional events, signed delivery and durable incoming receipts implemented | Atomic intent + at-least-once delivery | Jobs + signing/recipient policy; [jobs](jobs.md) |
| Scheduling | Intervals and timezone-aware calendar schedules implemented | UTC schedules enqueue deduplicated jobs | Durable jobs; [scheduling](scheduling.md) |
| Email templates + local capture | Optional mail adapter implemented; local capture/SMTP tests | Askama + Lettre + Mailpit | Delivery requirement; [email](email.md) |
| Durable email, provider feedback, notifications | Queued mail, inbox and preferences implemented; provider feedback is application-specific | Jobs/outbox, authenticated feedback, preferences | Jobs + identity/provider choice; [email](email.md), [communication](communication.md) |
| Outbound HTTP | Optional adapter implemented; local HTTP failure tests | Shared reqwest client with bounded timeouts | Destination trust; [communication](communication.md) |
| Cursor pagination and shared test fixtures | Implemented; notes runtime and upgrade tests | Typed forward cursor, bounded keyset query and timestamp/ID index | [Data](data.md), [architecture](../architecture.md) |
| Filtering and sorting | Implemented; shared typed allowlists, cursor binding and generated query parameters | SeaORM Select plus feature declarations | [Data](data.md) |
| Seeds, optimistic concurrency | Fixtures, idempotency and If-Match version checks implemented | Feature-specific commands and version rules | [Data](data.md) |
| Uploads/object storage | Scoped adapter, verified upload lifecycle, attachments and cleanup implemented | Private S3 objects + PostgreSQL metadata | Authorization + bucket; [uploads](uploads.md) |
| Search | Authorized PostgreSQL full-text reference implemented | PostgreSQL full-text search first | Search/language needs; [data](data.md) |
| Middleware, local rate/concurrency limits, CORS | Implemented; bounded Governor quotas and explicit route policies | Per-process budgets, configured origins | [Middleware](../middleware.md) |
| Caching and distributed quotas | Local cache implemented; distributed quotas remain a recipe | Moka for local cache; explicit shared-store/edge decision for global quotas | [Performance](performance.md) |
| Streaming/SSE, WebSockets | Retained SSE, bounded sockets and transient streams implemented | Axum streams/socket handlers | Authorization, backpressure/reconnect model; [realtime](realtime.md) |
| Metrics/distributed tracing | OTLP traces, HTTP propagation and protected bounded metrics implemented | OpenTelemetry to a collector | Telemetry backend; [observability](observability.md) |
| Container and CI | Implemented locally; remote hosting settings pending | Non-root image, shared check scripts | Docker/GitHub; [quality](../quality.md) |
| Deployment, secrets, backup/restore, migration recovery | Recipe; docs + container smoke only | Secret store, expand/contract, PostgreSQL PITR | Host, RPO/RTO; [operations](../operations.md) |
| Billing/domain integrations | Decision required | Select provider after money, tax, reconciliation, and webhook needs are known | [communication](communication.md); jobs + idempotency usually needed |

Dependency direction: identity → authorized uploads/private streams; jobs → scheduling and durable delivery; transaction + jobs → atomic outbox; email alone → synchronous local delivery only. None of these optional recipes is required for core startup.

When implementing a recipe, record the versions actually locked, new tests, real command output, and date. Promote only the implemented portion of a catalog row; one email trial does not validate other recipes or production email delivery.
