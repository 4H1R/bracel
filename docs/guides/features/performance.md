# Caching and rate limiting

Implementation update (2026-09-26): consult [the optional API package guide](../api-packages.md) for the current implemented workflows, package boundaries and limits. The design recipes below remain guidance for application-specific extensions; they are not verification evidence.

Bracel 0.2 provides optional ScopedCache with byte limits, TTL, scoped keys, concurrent load coalescing and invalidation. See [batteries](../batteries.md). Distributed invalidation and quotas remain recipes; local Governor policies remain implemented.

## Earlier recipe and further extensions

Local rate limiting is implemented as of 2026-09-25 using Governor 0.10.4. [Middleware](../middleware.md) is authoritative for shipped quotas, bounded key storage, concurrency limits, peer-IP handling and configuration. Tests exercise quota rejection, key capacity/cleanup, header preservation and probe exemptions. Caching and shared/distributed quotas below remain recipes; no Moka or Redis dependency was added.

## When and choice

Add a cache only after measuring repeated expensive reads and choosing acceptable staleness. PostgreSQL is sufficient for the default example. The optional cache recipe uses [Moka 0.12.16](https://docs.rs/moka/0.12.16/moka/) (`0.12`, `future`). Version checked 2026-09-25; the cache integration remains documentation-only. The local Governor integration is implemented in `bracel::http::middlewarerate_limit.rs`; extend that existing module rather than creating another limiter.

Multiple replicas require an explicit decision: local budgets multiply with replica count, and local invalidation does not propagate. Use a shared Redis service or an edge gateway for globally enforced quotas and coordinated cache state only when those semantics are needed. That introduces infrastructure, failure policy, TLS/auth and additional tests; do not quietly install Redis for this starter.

## Prerequisites and edits

Measure the target query. Decide cache capacity/TTL, authorization key space, negative caching, quota scope, burst and sustained limits, and fail-open/fail-closed behavior. Read [identity](identity.md) before caching private notes. Edit `Cargo.toml`, `src/config.rs`, `src/lib.rs::AppState` and `src/http/mod.rs::app`, and `src/features/notes/application.rs::get_note` and mutation functions. Create `src/cache.rs`, `src/rate_limit.rs`, `tests/performance.rs`. Initialize shared handles once in `src/main.rs::run`, never per request.

## Implement

1. Add Moka with a configured capacity (e.g. 1,000 entries) and TTL (e.g. 30 seconds). Define a key including tenant/user authorization scope, note ID, and representation version. Prefer caching the authorized query result; never return an unscoped cache hit before checking access. Cache successful reads only initially; bound item size as well as count.
2. In the note read application function, use Moka's async loading support to coalesce concurrent misses. Populate after a successful database result; invalidate after committed updates/deletes. A failed commit must not invalidate as if it succeeded. Document that TTL bounds staleness and does not provide immediate revocation: sensitive authorization changes must bypass/evict relevant caches.
3. Reuse the implemented Policies/Access route grouping in [middleware](../middleware.md). Governor quotas use peer IP before authentication and issuer/subject afterwards, with an additional write quota. Configure the shipped limits rather than installing a second local limiter. Trusted-proxy parsing is not implemented; forwarded headers are ignored.
4. Preserve the implemented 429/Retry-After and 503 capacity contracts, bounded key storage/idle cleanup, health exemptions and separate concurrency bound when extending policies. Test through the common stack so error normalization and CORS still cover denials.
5. For a shared backend, add explicit timeouts and a named outage policy. A security quota should generally fail closed or fall back to a conservative local allowance, while a cache may fall back to PostgreSQL. Prevent outage-induced stampedes with concurrency bounds. Test the selected behavior rather than relying on client-library defaults.

## Failures, tests, and operations

After creating tests run `cargo test --locked --test performance`, then both quality scripts. Test hit/miss/invalidation, TTL using a controlled clock where supported, bounded memory/key cleanup, concurrent miss coalescing, cross-tenant keys, authorization revocation, burst/refill, spoofed forwarded headers, 429 problem content/Retry-After, health exemption, and shared backend outage if selected. Add multi-replica tests before claiming a global quota. Benchmark against uncached reads so a cache does not disguise a missing index.

Monitor hit rate, evictions, load latency, limiter denials, backend failures, and database load after fallback; omit user IDs/IPs from metric labels. Deploy total capacity and budget proportional to replicas, or use a shared limiter. Rollback can disable the cache safely if the database is sized for the load; changing quota config may cause an immediate burst, so document the transition. Optional backend readiness should reflect the chosen failure policy, not automatically take the whole web service down.
