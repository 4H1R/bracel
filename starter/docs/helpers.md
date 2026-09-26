# Middleware, cache, authentication, jobs and schedules

## Where to customize

| File | Purpose |
| --- | --- |
| `src/middleware.rs` | Enabled common middleware |
| `src/cache.rs` | Cache settings and initialization |
| `src/features/accounts/auth.rs` | Current local user extractor |
| `src/jobs.rs` | Application job handlers |
| `src/schedules.rs` | Application calendar schedule definitions |
| `src/extensions.rs` | Registration markers used by the generator |

## Middleware

Edit the `ENABLED` array in `src/middleware.rs`. Remove `Middleware::Cors`,
`Timeout`, `BodyLimit`, `RateLimit`, `ConcurrencyLimit`, `Compression` or
`RequestContext` to disable that behavior. RequestContext groups request IDs,
safe logs, HTTP metrics, error normalization and the nosniff header.

The array selects layers; Bracel maintains execution order so CORS wraps early
rejections and request context observes the whole request. Duplicate entries
have no extra effect. Protected route authentication is controlled by
`RoutePolicy::Scope` and remains enforced when rate or concurrency middleware
is removed. Removing BodyLimit also disables Axum's default extractor limit.
Account login and reset quotas remain separate application rules.
Custom Axum middleware can still be attached to application or feature routers.

Compression additionally needs `--features compression` and
`HTTP_COMPRESSION=true`. It is independent of example API packages.
`inspect --json` reports selected middleware and route rate-policy settings.

## Cache

The default starter includes the `cache` feature. `AppState::from_config`
initializes one cache, and cloning state shares it. `--no-default-features`
omits cache and mail; either can be enabled individually.

| Setting | Default |
| --- | --- |
| `CACHE_CAPACITY_BYTES` | 16777216 (16 MiB) |
| `CACHE_TTL_SECONDS` | 60 |
| `CACHE_MAX_ITEM_BYTES` | 1048576 (1 MiB) |

The helper stores bytes and exposes `get(scope, key)`, `put(scope, key, value)`,
`forget(scope, key)` and `remember(scope, key, loader)`. All use the same keys,
expiration and size limits. Concurrent remember calls for a key share a load;
failed loads are not retained. `get_or_load` and `invalidate` remain available.

```rust,ignore
let scope = principal.cursor_scope();
state.cache.put(&scope, "profile", encoded_profile).await?;
let cached = state.cache.get(&scope, "profile").await;
state.cache.forget(&scope, "profile").await;
```

Derive private scopes from verified identity or membership. This cache is local
to each process; restarting clears it and replicas have separate values. It
does not provide Redis or shared invalidation. Authorization decisions must not
depend on a stale cached profile.

## Current user and principal

Use the starter's `CurrentUser` on a protected local-account route:

```rust,ignore
use bracel_starter::features::accounts::{CurrentUser, User};

async fn profile(CurrentUser(user): CurrentUser) -> axum::Json<User> {
    axum::Json(user)
}
```

Register that route with `RoutePolicy::Scope("account:self")`. The extractor
requires the verified principal, checks the local-account scope and issuer,
then loads the existing user. The shipped `/api/users/me` endpoint uses it.
User deletion is observed immediately; session expiry and revocation are
checked by the authentication policy before extraction.

For external JWTs or code that needs issuer, subject and scopes, accept
`principal: bracel::identity::Principal` directly as an Axum extractor. Missing
verified identity returns 401 with a Bearer challenge. Credentials supplied to
public routes are not implicitly authenticated. Local login, registration,
logout and reset behavior is described in [accounts](accounts.md).

## Jobs and cron

Register application handlers in `src/jobs.rs`, or generate a handler with
`bracel make job NAME`, which registers through `src/extensions.rs`. The
framework's `TypedJob`, `dispatch` and `DispatchOptions` support transactional
dispatch, queue names, delays, priority and bounded attempts. PostgreSQL holds
the queue; handlers must tolerate at-least-once delivery.

```bash
bash scripts/dev.sh migrate
cargo run -- schedule:sync
cargo run -- schedule:work
# Separate process:
cargo run -- jobs:work
# Or bounded parallel execution of one queue:
cargo run -- jobs:parallel default 4 25
```

Export the database environment before using `cargo run`; that command does
not source `.env`. `jobs:once`, `jobs:failed` and `jobs:retry ID REASON` remain
available. Workers and schedulers are separate long-running processes.

`src/schedules.rs` contains cron definitions, starting with hourly account
cleanup. Expressions include seconds: `0 0 * * * *` means every hour. Each
definition supplies an IANA timezone and a job with a registered handler.
The worker performs bounded cleanup per run; tune cadence for your workload.

Run `schedule:sync` explicitly after editing definitions. In one transaction it
updates/re-enables listed schedules while preserving their due times and
disables removed schedules in the reserved `app.` namespace. Other calendar
schedules are left alone. Editing a definition does not run it immediately.
`schedule:tick` runs interval and calendar polling once; `schedule:work` polls
both continuously in the default build. No batteries feature is required.

Calendar polling coalesces missed occurrences, skips enqueueing while the last
job is pending/running, and disables exhausted finite expressions. Queue lease
expiry can still cause an external effect to repeat. Review civil-time
schedules at DST transitions. The new additive migration installs queue and
calendar objects in the normal starter and tolerates existing optional-package
installations.
