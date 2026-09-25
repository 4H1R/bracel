# Durable jobs, outbox, events, and webhooks

## When and choice

Use durable jobs when work must survive restarts or may exceed a request deadline. An in-process Tokio task is sufficient only for explicitly disposable work. Recommend a small PostgreSQL lease queue using existing SeaORM/SQL transactions: no broker, same atomic boundary as notes, visible operational state. Keep this queue specific to the implemented task types; adopt a specialist queue if throughput, workflows, or operations outgrow it.

Versions/source checked 2026-09-25: PostgreSQL 18 [locking/`SKIP LOCKED`](https://www.postgresql.org/docs/18/sql-select.html#SQL-FOR-UPDATE-SHARE), SeaORM 2.0.3 [transactions](https://www.sea-ql.org/SeaORM/docs/advanced-query/transaction/). No additional queue library is needed. **Documentation-only, no worker executed.** This is an implementation design requiring concurrency tests, not a tested queue subsystem.

## Prerequisites and edits

Define acceptable delay, retry limit, idempotency, retention, and task payload version. Read [database](../database.md); use [email](email.md) or [outbound HTTP](communication.md) for external effects. Edit `src/migrations/mod.rs::Migrator`, `src/features/notes/application.rs::create_note`, `src/config.rs`, and `Cargo.toml` if a new payload type needs libraries. Create `src/features/jobs/mod.rs`, `src/features/jobs/entity.rs`, `src/bin/worker.rs`, `tests/jobs.rs`, and `scripts/jobs-test.sh`. Export the module in `src/lib.rs`; the worker shares `db::connect`, safe logging, and shutdown conventions from `src/main.rs`.

## Implement

1. Add `jobs`: UUID id, kind and payload_version, minimal JSON payload, unique dedupe_key, available_at, attempt count, lease_token UUID, lease_until, status (pending/running/succeeded/failed), safe last_error_code, created_at/finished_at. Add an index over due pending jobs and expired leases. Store references rather than full sensitive content. A business event may be the job itself; a separate outbox table is needed only when event fan-out or retention differs.
2. Add `create_note_and_enqueue` beside `create_note`: begin a transaction, call the existing `create_note(&transaction, input)`, insert the job intent on that same transaction, then commit. Update the HTTP handler to call this composed operation. `create_note` already accepts SeaORM's `ConnectionTrait`; reuse it without a new repository abstraction or nested transaction. No email/network call occurs inside the transaction. On conflict of a dedupe key, confirm that the existing intent matches the requested operation. Return the note after commit; HTTP success means accepted intent, not completed delivery.
3. Claim a bounded batch in a **short transaction** using `SELECT ... FOR UPDATE SKIP LOCKED` followed by `UPDATE ... RETURNING`. Use database time. Reclaim expired leases. Increment attempt count on each claim, set a fresh token and lease, and commit before performing any network work. A worker should limit concurrency to a small configured number; empty polls sleep with jitter.
4. Dispatch a concrete enum of versioned payloads. Wrap each effect in its integration timeout. Set lease duration above that bound (for example 30-second lease for a 5-second SMTP call). Long jobs need token-conditional heartbeats. Acknowledge or reschedule with `WHERE id = ? AND lease_token = ? AND status = 'running'`: stale workers must not overwrite a new claim. Persist success before claiming more.
5. On transient failure, use exponential backoff with jitter (start 5s, cap 15m, at most 8 attempts). Persist `available_at`; don't hold a thread asleep for a retry. Mark permanent or exhausted work failed with a safe reason. Expose an administrator CLI to list by safe metadata and replay a selected failed ID with a fresh attempt budget and audit reason. Replays preserve the business idempotency key. Never add a public replay endpoint without authorization.
6. Stop claiming on SIGTERM; finish or abandon bounded tasks, then close the pool. Unacknowledged leases recover after expiry. Add a worker deployment entry with its own pool/concurrency budget, not an HTTP background loop in every web replica.

## Failure semantics and verification

This is **at least once**. If delivery succeeds and acknowledgement fails, the job repeats. Use provider idempotency keys when available, or a transactionally unique effect record for database-only work. SMTP cannot guarantee deduplication, so duplicate email remains possible. Poison payloads fail visibly; unknown versions must not crash the process or retry forever. Never equate lease ownership with exactly-once external effects.

After implementing `tests/jobs.rs`, run `cargo test --locked --test jobs` and the core checks. Test two simultaneous workers claiming distinct jobs; restart between claim/effect/ack; lease expiry and stale acknowledgement; transaction rollback leaving neither note nor job; dedupe conflict; backoff bounds; permanent failure; replay; and graceful shutdown. Use real PostgreSQL with isolated schemas as in `tests/http.rs`. Add a CI test starting/stopping the built worker, and integration tests for each effect. A worker ping alone does not prove useful progress.

Operate with queue depth, oldest due age, failed count, lease expirations, and worker heartbeat. Readiness should include the worker's database, not a transient provider failure. Inspect safe error codes and task IDs; keep PII out of logs and minimize payload retention. Deploy backward-compatible payload readers before writers. Roll back workers only while they can understand queued versions; pause claims during schema repair. Add scheduled retention cleanup and alert on stalled work.

## Events and webhooks

For database-triggered domain events, insert the event/intent in the business transaction and fan out through jobs. Each consumer needs a unique `(consumer, event_id)` record committed with its database effects. Do not publish first and then hope the database commits.

Inbound webhooks require provider signature verification over the exact raw bytes, size/time bounds, timestamp/replay checks, and a unique provider event ID. Persist the verified intent before returning 2xx, then process asynchronously. Outbound webhooks use HTTPS destinations approved at subscription time, SSRF controls (including DNS/IP and redirects), HMAC signatures with timestamp, event IDs, a bounded timeout, and the same retry/failed/replay workflow. Return 410 or disable delivery only under an explicit subscription policy. Test invalid signatures, duplicate delivery, reordering, slow endpoints, key rotation, and recovery; see [communication](communication.md).
