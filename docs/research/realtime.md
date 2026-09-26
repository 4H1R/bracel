# API-only realtime for Bracel

Research checked 2026-09-26. These are implementation recommendations, not shipped capabilities. “Mercury” is interpreted as [Mercure](https://mercure.rocks/spec); confirm that interpretation before pursuing protocol compatibility. No HTML rendering or frontend runtime is needed.

## Current baseline

The [realtime recipe](../../starter/docs/features/realtime.md) explicitly says no realtime endpoint ships. [Axum configuration](../../crates/bracel/Cargo.toml) does not enable WebSockets. [Jobs](../../crates/bracel-jobs/src/lib.rs) provide transactional work enqueueing and competing workers, not subscriber fan-out or retained stream replay.

[Principal](../../crates/bracel/src/identity.rs) contains issuer, subject and scopes. JWT expiration is validated on entry but is not retained in the returned principal. [Middleware](../../crates/bracel/src/http/middleware/mod.rs) releases its concurrency permit when the handler returns the response, and its CORS allowlist excludes `Last-Event-ID`. Both need stream-specific additions. Route scopes alone cannot authorize a requested topic or tenant.

## Transport choice

| Option | What it supplies | Recommendation |
| --- | --- | --- |
| Axum SSE | Streaming HTTP response and heartbeat helpers; SSE defines IDs and reconnect requests | Default for notifications, resource changes, job progress and AI output |
| Axum WebSockets | Bidirectional socket upgrade, frame/message/write-buffer bounds | Optional feature for genuinely bidirectional sessions; reuse authorization and event contracts |
| External Mercure hub | Topic publication/subscription, JWT grants, history reconciliation and hub operations | Later adapter for teams choosing dedicated realtime infrastructure |

Axum supplies transport primitives, not Bracel's topic policy, persistence or reconnect guarantees. [Axum SSE](https://docs.rs/axum/0.8.9/axum/response/sse/), [WebSocketUpgrade](https://docs.rs/axum/0.8.9/axum/extract/ws/struct.WebSocketUpgrade.html).

Mercure specifies topic authorization and history reconciliation; it permits discarded history and recommends refetching when a replay gap is detected. Native EventSource does not expose the response header used for its gap check. [Mercure specification](https://mercure.rocks/spec).

The current vendor hub documentation distinguishes open-source single-node deployment from licensed multi-node transports, including PostgreSQL. An adapter therefore has operational and licensing choices; implementing SSE in Rust does not imply Mercure protocol compatibility. [Mercure deployment options](https://mercure.rocks/docs/production/high-availability).

## Recommended implementation

Add an optional `bracel::realtime` module first; application state, routes, migrations, event DTOs and topic authorization remain application-owned. Define a small typed event envelope containing stable ID, event type, schema version, topic, payload and correlation ID. Keep realtime event IDs separate from resource pagination cursors.

1. **Durable publication:** write event intent in the same PostgreSQL transaction as the resource change. Retain replayable events independently of job completion. Each API replica tails the shared log and fans out only to its own authorized connections; a competing job queue must not consume the single copy of each event on behalf of all subscribers.
2. **Ordered delivery:** use a committed dispatch log whose append position is serialized through a database lock/counter held until commit. A dispatcher reads pending committed intents, appends idempotently and marks them dispatched in one transaction. This orders publication, not necessarily domain changes; include aggregate revisions when consumers require that ordering. Do not advance a replay cursor using raw `BIGSERIAL`, UUIDv7 or creation timestamps allocated in concurrent business transactions.
3. **Wake-up only:** use PostgreSQL `LISTEN/NOTIFY` to reduce polling delay, with periodic catch-up from durable rows. Commit `LISTEN` before initial catch-up and repeat catch-up after reconnect. Notifications target listening sessions and are not a durable replay store. Keep notification payloads minimal. [PostgreSQL NOTIFY](https://www.postgresql.org/docs/18/sql-notify.html), [LISTEN startup race](https://www.postgresql.org/docs/18/sql-listen.html).
4. **Replay contract:** document retention, duplicate delivery, maximum catch-up and snapshot recovery. Validate cursors against the authenticated subscription. An expired cursor must produce an explicit `resync_required` outcome; do not silently pretend replay is complete. Obtain snapshot and cursor consistently to avoid a snapshot-to-subscription gap.
5. **Connection contract:** bounded subscriber queues, disconnect slow readers, heartbeats, connection-lifetime quotas, idle/max-lifetime limits, deployment drain, reconnect backoff and payload redaction. Stream errors after headers use typed control events/close reasons; handshake errors retain Problem Details. Register event schemas and operational limits in inspection output.

The ordering hazard above is an inference from PostgreSQL sequence allocation semantics: transaction A can allocate 1, transaction B allocate 2 and commit first; a client advancing to 2 then misses A's later commit. Sequences are neither rollback-safe counters nor commit-order markers. [PostgreSQL sequence functions](https://www.postgresql.org/docs/18/functions-sequence.html).

## Authentication and browser compatibility

Keep bearer headers for machine/mobile clients. Browser native EventSource and WebSocket constructors expose no arbitrary Authorization-header option. Recommend fetch-based SSE for bearer browser consumers; document reconnect parsing and cancellation. API-only describes the server, not a ban on browser API consumers. [EventSource standard](https://html.spec.whatwg.org/multipage/server-sent-events.html), [WebSocket standard](https://websockets.spec.whatwg.org/).

Add authenticated connection context with credential expiration and a revocation/revalidation handle. Authorize every topic subscription and recheck mutable membership on a bounded schedule; close at expiry. Never put durable bearer tokens in URLs. For future browser WebSockets, explicitly design short-lived single-use tickets or bounded first-message authentication with no data delivery beforehand; validate Origin independently of authentication.

## Delivery and evidence

Implement durable events plus SSE first, then WebSockets sharing those contracts, then an optional Mercure publisher adapter. AI runs should expose durable run status/final results; token deltas can be explicitly ephemeral to avoid one durable database write per token.

Before implementation, write E2E failure scenarios: rollback, cross-owner topics, expired/revoked credentials, concurrent commit ordering, lost wake-ups, two replicas, replay gaps, duplicate delivery, slow readers and SIGTERM. Exercise real sockets, PostgreSQL and two API processes. Produce a repeatable script plus retained JSON scenario results, event transcript, versions and redacted logs. This research ran no implementation tests and makes no runtime-verification claim.
