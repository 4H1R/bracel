# Scheduling durable work

## When and choice

Use this for periodic cleanup, reports, or retries tied to business time. Existing HTTP routes suffice for manual requests. Recommend a PostgreSQL-backed UTC schedule that inserts jobs using the [jobs recipe](jobs.md). A Tokio interval may wake the scheduler, but persisted due times and unique occurrences supply durability. For a single operational backup, the deployment's managed scheduler can invoke a CLI; still make the operation idempotent.

Checked 2026-09-25: PostgreSQL 18 locking and timestamp arithmetic; [cron 0.17.0](https://docs.rs/cron/0.17.0/cron/) (`0.17`) if cron expressions are actually required. Prefer fixed UTC intervals first; do not add cron just for one hourly cleanup. **Documentation-only; scheduler/concurrency behavior has not been executed.**

## Prerequisites and edit map

Implement durable jobs first. Decide missed-run policy (default: coalesce to one catch-up run), time zone (default UTC), maximum lateness, and overlap policy. Edit `src/migrations/mod.rs::Migrator`, `src/config.rs`, and future `src/features/jobs/mod.rs`/`src/bin/worker.rs`. Create `src/features/jobs/scheduler.rs` and `tests/scheduling.rs`; export from the jobs module. Schedule polling can run in each worker if locking/deduplication are tested. Do not attach it to request handlers.

## Implement

1. Add a schedules table: stable key, payload version, enabled, interval/cron definition, next_due_at (`timestamptz`), and last_enqueued_at. Seed known schedules idempotently from an explicit deployment command; never silently reset next_due_at on every restart.
2. Poll at a bounded interval (e.g. 5 seconds); select due schedule rows with `FOR UPDATE SKIP LOCKED` in a transaction. Calculate due occurrences from persisted database time. Insert a job with unique dedupe key `schedule-key:scheduled-instant`, and advance next_due_at **in the same transaction**. Commit before another poll.
3. For the default coalescing policy, enqueue at most one missed occurrence per schedule, advance to the next future instant, and record the lateness/coalescing decision. If the product needs every missed occurrence, bound catch-up batches and capacity. For “no overlap,” use a per-schedule execution lease or a job uniqueness predicate; scheduler row locks alone end before job execution.
4. Implement a first task such as deleting expired sessions in bounded batches. Authorization/deletion retention rules still apply. Do not hardcode a new retention policy into the scheduler. Cancel polling on the worker's existing shutdown signal.

## Failure, tests, and operations

Two schedulers can race; the row lock and unique job key must yield one intent. A crash after insert but before commit produces neither job nor advanced schedule. A committed job can still execute twice under the jobs semantics. Clock changes use database UTC; local-time schedules require explicit daylight-saving rules (skip or repeat), tested in both transitions. Invalid schedule definitions should fail configuration with a safe key, not spin or stop unrelated schedules.

After creating tests, run `cargo test --locked --test scheduling`, `cargo test --locked --test jobs`, then `bash scripts/check.sh`. Inject a clock into the scheduling calculation and test actual PostgreSQL transactions: simultaneous scheduler instances, downtime catch-up, disabled schedules, definition changes, overlap, crash/rollback, and restart. CI must run both scheduler and worker behavior. No external account is needed.

Expose last tick, due lag, coalesced count and failed-job age. Alert on missing scheduler progress, not merely process liveness. Deployment needs a worker with permissions for schedules/jobs and a graceful stop budget. Rollback preserves schedule keys and payload versions; pause a schedule before repair rather than deleting occurrence history. Document changes to cadence and retention so operators can replay missed work deliberately.
