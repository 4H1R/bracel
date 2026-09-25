# Outbound HTTP, feedback, and notifications

Documentation-only guidance, authored 2026-09-25. No outbound client exists in the core. Checked [reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/) (`0.13`, disable defaults and choose Rustls/JSON features appropriate to the integration). Authentication's openidconnect integration uses its compatible reqwest 0.12 line separately.

## Outbound HTTP

Create `src/outbound.rs`; edit `Cargo.toml`, `Config`, `AppState`, and `src/main.rs::run` to initialize one pooled client. Set 2-second connect and 5-second total request timeouts, disable automatic redirects for untrusted destinations, set a clear User-Agent, and bound response bytes while streaming (do not call unbounded `bytes()` on an untrusted response). Validate HTTPS destinations against an allowlist when possible. User-controlled URLs require DNS/IP validation, blocked loopback/private/link-local ranges, redirect validation, and network egress controls to handle DNS rebinding. Avoid proxy environment surprises in sensitive fetchers.

Use a feature-specific adapter that maps status codes to safe domain errors, never raw provider bodies. Retry only transient failures within a total budget, honor Retry-After, add jitter, and reuse provider idempotency keys for retryable writes. A timeout can mean the provider accepted a write; surface ambiguous outcomes or reconcile them. Credentials live in secret-backed config and are redacted. Do not log authorization headers, signed URLs or query tokens.

Create `tests/outbound.rs` with a local stub server. Test stalled connection/body, oversized responses, redirects, 429/503 retry bounds, permanent failures and duplicate writes. Run `cargo test --locked --test outbound` plus normal checks; CI must not depend on the public internet for application tests. Track latency/failures by a small provider enum and expose dependency outage separately from liveness.

## Provider feedback and notifications

Use [email](email.md) for SMTP/template delivery and [jobs](jobs.md) for reliable feedback processing and event replay. Choose channel preferences (email, push, in-app), delivery urgency and suppression policy first. Add `src/features/notifications/` and migrations for notification intent/status and preferences only when requested. A notification ID is distinct from the underlying business event and each channel attempt. Insert intent atomically with the relevant event; enforce current authorization/preferences at dispatch. Avoid copying sensitive message content into logs or generic event payloads.

Feedback webhooks authenticate raw bytes, validate timestamp, and deduplicate provider event IDs before asynchronous processing. A bounce updates suppression state; a delivery receipt does not prove a user read it. Prevent out-of-order events from reverting terminal states. Test signature failures, replay, duplicate/out-of-order feedback, preference changes and provider outages with recorded non-sensitive fixtures. Schedule retention cleanup and monitor delivery latency/backlog.

Billing and domain providers remain **decision required**: identify merchant/legal ownership, supported countries/currencies, recurring billing model, reconciliation, idempotency and refund semantics before selecting an SDK. Reuse signed webhooks and durable jobs, but record a separate project decision and sandbox tests. No provider credentials or real messages belong in normal starter checks.
