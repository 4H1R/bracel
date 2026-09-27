# Starter helper acceptance cases

Defined 2026-09-27 before implementation.

- Middleware: default requests retain correlation IDs, CORS, deadlines and body limits. Removing a layer changes that behavior. Disabling rate/concurrency limits must not disable authentication on a protected route. Inspection must report the enabled list accurately.
- Cache: put/get/forget and remember share the same scoped keys. Different users cannot share values accidentally. Oversized writes fail; loader errors are not cached; concurrent remember calls coalesce; expired entries miss. One application state shares one cache across requests, with validated limits.
- Auth: CurrentUser requires a verified local principal and an existing user. Missing credentials, external issuers, deleted users and revoked sessions cannot resolve a current user. The profile endpoint exercises this extractor.
- Jobs/schedules: the default build supports typed jobs, named queues, parallel workers and calendar ticks without enabling example API packages. Explicit schedule sync installs application definitions without resetting due times. Tick coalesces overdue occurrences, does not enqueue another pending/running occurrence, and the normal worker processes the registered handler. Fresh and previously migrated databases both support the schema; repeated migration is harmless.
- Distribution: framework and standalone starter sources agree. Default, no-default-feature and all-feature builds compile. HTTP/account E2E checks and PostgreSQL job checks retain their existing contracts.

Execution results belong in verification.md; this list is not a passing-test claim.
