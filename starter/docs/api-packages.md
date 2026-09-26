# Optional API workflows

Bracel is an API framework. These packages add JSON, file, SSE and WebSocket workflows; they do not add HTML rendering or a frontend.

Compile the reference workflows with `cargo build --features batteries,identity,telemetry`. Enable `ENABLE_BATTERIES=true` with `AUTH_MODE=bearer`. Run the explicit `migrate` command before serving. The application owns migration history; constructing a package handle never changes the database. Existing applications adopt the new package SQL in their own migration rather than replacing earlier migrations.

After applying the reference batteries migration, keep its migration module compiled for that database. Disable endpoints with `ENABLE_BATTERIES=false`; removing the compile feature also removes that reference migration from the binary's history. Adopters should retain their application's migration definitions independently of later endpoint choices.

`bash scripts/dev-all.sh` builds and migrates, then runs the API, application worker and scheduler. With batteries enabled it also polls incoming deliveries, plus configured mail/webhook queues. Ctrl-C stops the child processes. PostgreSQL must already be available.

## Ownership

| Crate | Responsibility |
| --- | --- |
| `bracel` | HTTP lifecycle, shared errors/results, request validation, allowed queries, pagination, route contracts, bearer verification, policies and middleware |
| `bracel-jobs` | PostgreSQL queue, typed jobs, retries, delayed/named queues, concurrency and calendar schedules |
| `bracel-data` | Atomic idempotency records, optimistic versions, membership context and transactional audit |
| `bracel-realtime` | Transactional events, consumer deduplication, retained streams, SSE, optional WebSockets and transient JSON streams |
| `bracel-delivery` | Notification inbox, preferences, queued mail, signed webhooks and durable incoming receipts |
| `bracel-files` | Scoped upload metadata, verification, attachments, deletion and cleanup |
| `bracel-integrations` | Independently enabled SMTP, object storage, cache, HTTP, configured OIDC/JWKS and OTLP adapters |

The starter owns route paths, project records, permission names, configured destinations and process startup. Packages accept explicit state and transactions. `bracel::jobs` remains a compatibility re-export of the jobs package. Domain modules need no service container or universal resource base class.

## Requests and resources

`Schema` declares required, optional and nullable fields with text/integer bounds, UUIDs, RFC3339 timestamps, decimal strings, enums, nested objects and arrays. `UniqueJson` rejects duplicate object keys at every depth. PATCH validates only supplied fields and preserves explicit null. Use `ValidationErrors` for application cross-field rules; database uniqueness remains a database constraint.

`POST /api/projects` requires `projects:write` and `Idempotency-Key`. The principal, operation and key identify a replay record. A changed request conflicts; concurrent identical requests serialize. Resource, audit, event, delivery intent and successful response commit together. Failure rolls them all back. Replay lasts 24 hours; run `idempotency:cleanup` to remove at most 500 expired records per call. External delivery is at least once and requires provider idempotency/reconciliation when effects cannot be repeated.

`PATCH /api/projects/{id}` requires `If-Match: "1"` for version 1. Missing preconditions return 428 and stale versions return 412. DELETE is a version-checked soft deletion; POST to `/{id}/restore` restores it. Reads never expose another owner's records.

Collections support bounded before/after cursors, offset up to 10,000, limits up to 100, boolean/status filters, PostgreSQL full-text `q`, field selection and an allowed `audit` include loaded in one extra query. Includes use a bounded audit collection. Cursors bind the access/filter context. The reference orders by stable IDs; relevance-ranked search is application-specific. `POST /api/projects/import` accepts at most 50 validated records and commits the whole batch under one idempotency key. Large exports should traverse cursor pages; snapshot recovery is bounded to 1,000 records and explicitly fails above that limit.

Tenant context is opt-in. `TenantScope::resolve` checks the authenticated principal's stored membership; `require_write` and `require_admin` compose with route and record policies. The operator CLI `tenants:grant TENANT ISSUER SUBJECT reader|writer|admin` bootstraps membership. `/api/tenants/{id}/projects` demonstrates shared tenant reads and writes. Applications pass the resolved tenant key to files, caches and delivery, and recheck membership for delayed work. Owner event subscriptions deliberately cannot use a tenant key without an application-provided authorization design.

## Events and streaming

Publish a typed `DomainEvent` inside the business transaction. Retained events have a UUID, kind, version, topic and JSON data. A PostgreSQL counter serializes publication through commit, making resume positions safe across replicas. This favors correctness and modest throughput; publication shares one counter lock. Subscribers poll every 250ms without holding a database connection between queries.

`GET /api/events?topic=projects` requires `events:read`. Reconnect using `Last-Event-ID`. A cursor from another owner/topic is rejected. Pruned history returns 409 before streaming; use `/api/projects/snapshot` and resume at its consistent cursor. `events:prune POSITION` removes at most 1,000 positions per call. A live stream error sends `resync_required` and closes. Client processing must deduplicate event IDs.

Connections have bounded batches and channels, 64KiB event payloads, a one-hour session limit, heartbeat, slow-send timeout and process shutdown cancellation. Credentials are reverified while connected, including machine-token revocation. Quotas are process-local. Deployments need a proxy idle timeout that permits the heartbeat and must disable proxy buffering for SSE.

WebSocket support is optional on `bracel-realtime`. It uses the same events, authorization and replay. `websocket_with` rechecks the principal before each bounded JSON message; application handlers enforce message-specific policies. The starter accepts `{"type":"projects.patch","id":"UUID","version":1,"patch":{"name":"Updated"}}` only with `projects:write`, and uses the same owner/version checks as HTTP. Invalid commands receive a bounded `message_rejected` response. Origins must match configured CORS origins when supplied. Native ping/pong and `{"type":"ping"}` are supported. Browser clients need a bearer-capable transport or an application-specific connection-ticket design; tokens in query strings are not supported.

`bracel_realtime::transient(source, lifetime)` serves bounded JSON chunks as SSE for model output or progress. It does not persist chunks. Disconnect drops the source, and deadline/error ends the stream. The application owns authorization and cancellation of any independent producer task. No model-provider SDK is required.

## Delivery and files

Notifications have a durable owner/tenant inbox and read state. Mail and realtime preferences are checked. `enqueue_mail` deduplicates the complete delivery intent and queues delivery in the same transaction. Local reference SMTP uses `MAIL_LOCAL_PORT` and `NOTIFY_EMAIL`; production applications construct `Mailer` with their SMTP configuration. SMTP acceptance is recorded, permanent rejection fails the job, and transient errors retry. Acceptance is not proof of recipient delivery. Use the adapter's message builder for multipart/attachment composition; provider feedback events are application-specific.

Outgoing webhooks use administrator-configured `WEBHOOK_URL`/`WEBHOOK_SECRET`, stable delivery IDs and HMAC-SHA256 over timestamp, ID and exact bytes. Retryable failures include 408, 429 and 5xx. Arbitrary user-supplied destinations are not accepted by this example. Incoming requests require `WEBHOOK_INCOMING_SECRET`; signatures expire after five minutes and replayed IDs are deduplicated. Receipt and `IncomingJob` enqueue are atomic. `Incoming::begin` locks the receipt and `complete` commits application effects with processed state. The starter incoming worker marks receipt processing complete; replace its handler with domain effects. `delivery:once mail|webhooks|incoming`, `jobs:failed` and `jobs:retry ID REASON` provide inspection/recovery without a dashboard.

Set `FILES_ROOT` for the local reference routes. Initialize metadata, upload bytes, complete, then download or attach. Initialization accepts at most 1MiB in the starter; the package supports configured bounds up to 16MiB. Completion verifies size, SHA-256 and basic type signatures before copying to an immutable final key. This is not malware scanning. `Files::signed_url` supports S3 PUT/GET grants for applications configuring the S3 adapter; local storage uses authenticated proxy routes. Upload grants last five minutes and must expire before the reservation. Completion and deletion reconcile both staging/final keys. Run `files:cleanup` repeatedly; it retries tombstones and expired uploads and removes staging objects after outstanding grants expire. Database and object storage cannot commit atomically, so reconciliation remains necessary.

## Jobs, identity and operations

Implement `TypedJob`, register explicitly and use transaction-aware `dispatch`. Dispatch options include named queue, delay, priority and maximum attempts. `jobs:parallel QUEUE CONCURRENCY TIMEOUT_SECONDS` runs 1–64 application handlers with bounded deadlines. Workers stop claiming on shutdown and drain their current bounded attempts; leases are fenced by the existing job claim token. Long work should checkpoint into smaller jobs rather than exceed the 3,590-second maximum attempt deadline.

`schedule:calendar NAME EXPRESSION TIMEZONE` accepts cron's seconds-first expression and IANA timezone. Missed occurrences coalesce, an unfinished previous job prevents overlap, and exhausted finite schedules disable after their final occurrence. Calendar ticking runs alongside interval schedules when batteries are enabled. Schedule timezone behavior follows the pinned cron/chrono-tz implementation; operators should review civil-time schedules around DST. Chains and batches are outside this initial queue API.

`AUTH_DISCOVERY_URL` must exactly match `AUTH_ISSUER`; set `AUTH_AUDIENCE` and omit static keys. Discovery and JWKS must share the configured issuer's origin. Only RS256 access tokens with `typ=at+jwt` are accepted. Refresh runs every 30 seconds; a successful key set is usable for at most five minutes without refresh, after which verification fails closed. Unknown keys wait for refresh. Provider accounts retain their provider-managed lifecycle. The starter separately includes [local API accounts](accounts.md); MFA is not implemented.

`TRUSTED_PROXY_CIDRS` configures explicit proxy CIDRs before forwarded client addresses affect rate keys. `HTTP_COMPRESSION` controls optional gzip; SSE is excluded. `OTLP_ENDPOINT` enables HTTP/database mutation/job/outbound spans and W3C HTTP propagation. Queued jobs have their own spans; cross-process parent propagation is not implicit. `/api/operations/metrics` requires `ops:read` and returns bounded HTTP labels plus queue counts. Redis/shared quotas, query-budget enforcement and OTLP metric export are not implied by these local counters.

`inspect --json` reports the registered routes and supported/compiled/configured capabilities. Provider verification remains `not_checked` unless an actual acceptance run supplies evidence. `docs/openapi-packages.json` is the compiled optional contract; it does not imply routes are enabled in a running deployment.

## Development and evidence

The CLI adds `make job`, `event`, `policy`, `command` and `migration`, with dry-run JSON plans and conflict protection. Registration is explicit in `src/extensions.rs`; generated unfinished jobs/commands/migrations fail visibly. Resource fields include `string`, `i64`, `bool`, `uuid`, `date`, `decimal`, `enum(draft|ready)` and nullable `?` suffixes, plus transactional PATCH. Edit generated domain code and its schemas as requirements grow.

Run the framework workspace's `bash scripts/api-e2e.sh` with a disposable `TEST_DATABASE_URL`. Scenarios use real API processes, PostgreSQL, SMTP/HTTP peers and WebSocket frames. Reports, redacted transcripts, logs, commands, service versions and source hashes are retained under `.scratch/api-e2e/`; generated-application evidence is under `.scratch/generated-e2e/`. A recipe or a compiled provider adapter is not evidence of a live cloud integration.
