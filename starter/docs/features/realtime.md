# Streaming, SSE, and WebSockets

Documentation-only guidance, authored 2026-09-25 against [Axum 0.8.9 SSE](https://docs.rs/axum/0.8.9/axum/response/sse/index.html) and [WebSockets](https://docs.rs/axum/0.8.9/axum/extract/ws/index.html). No realtime endpoint is shipped.

Use SSE for one-way server notifications and WebSockets only for bidirectional interaction. Existing request/response routes suffice for occasional reads. Decide authorization, event retention/replay, cross-replica routing, reconnect policy and maximum connections first. In-process channels can deliver ephemeral signals; reliable events require the [outbox/jobs](jobs.md) design or durable storage.

Create `src/features/notifications/stream.rs` and `tests/realtime.rs`; edit `src/http/mod.rs::app`/`AppState`, `src/main.rs::shutdown`, `Config`, and `Cargo.toml` (Axum's `ws` feature only when sockets are needed). Add a shared shutdown signal and bounded channel per subscriber. Authenticate before upgrade/subscription and recheck session expiry/membership during long-lived connections. Validate WebSocket Origin. Never broadcast unfiltered tenant data and rely on clients to ignore it.

SSE should emit stable event IDs, explicit heartbeat intervals (e.g. 15s), and a documented Last-Event-ID replay window. Drop or disconnect slow subscribers according to a bounded queue policy; reconnect must recover via an HTTP snapshot plus cursor if ephemeral events were lost. WebSockets need frame/message size limits, idle/ping timeouts, concurrency limits and explicit close reasons. Parse message types into a small validated enum.

The core request deadline covers obtaining a response, not the lifetime of a streaming body or upgraded socket. Add stream-specific lifetime/idle limits and shutdown propagation rather than disabling deadlines across all routes. Configure proxy buffering/idle timeout and deployment drain budgets explicitly. Never put access tokens in URLs. Logs should record event categories/counts, not private payloads.

After implementation run `cargo test --locked --test realtime` and both core scripts, adding a container-level connection test to CI. Cover cross-tenant isolation, unauthenticated/expired session, slow reader memory bounds, heartbeat, reconnect/replay, dropped events, malformed/oversized frames, connection quotas and SIGTERM closing active streams. Monitor active connections, queue drops and reconnect rate with bounded labels. Rollback must preserve event versions or force a documented resynchronization.
