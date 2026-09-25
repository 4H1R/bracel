# Metrics and distributed traces

## When and choice

The core's JSON request logs and probes are enough to diagnose a small local service. Add metrics for trends/alerts and traces when requests cross service boundaries. Recommend OpenTelemetry OTLP to a collector, keeping the existing tracing spans and JSON logs. The collector owns vendor export and buffering; the application does not require vendor credentials for normal tests.

For authoritative records of sensitive business actions, follow the [identity recipe's durable audit guidance](identity.md#durable-audit-history-when-required). An exporter that can drop telemetry does not satisfy the transaction and retention requirements of an audit history.

Checked 2026-09-25: [opentelemetry 0.33.0](https://docs.rs/opentelemetry/0.33.0/opentelemetry/), [opentelemetry_sdk 0.33](https://docs.rs/opentelemetry_sdk/0.33.0/opentelemetry_sdk/), [opentelemetry-otlp 0.33.0](https://docs.rs/opentelemetry-otlp/0.33.0/opentelemetry_otlp/), [tracing-opentelemetry 0.34.0](https://docs.rs/tracing-opentelemetry/0.34.0/tracing_opentelemetry/) (its dependency explicitly targets OpenTelemetry 0.33). Use this matched set, not equal version numbers across every crate. **Documentation-only; collector export and SDK integration not compiled or executed.**

## Prerequisites and edits

Decide service name, environment attributes, collector endpoint, sampling, retention and alert ownership. Read [HTTP logging/redaction](../http.md) and [operations](../operations.md). Edit `Cargo.toml`, `src/main.rs::main`/`run`/`shutdown`, `src/config.rs`, and `src/http/mod.rs::request_context`. Create `src/telemetry.rs`, `tests/telemetry.rs`, and a local collector configuration/Compose override. No new database migration is needed.

## Implement

1. Add the matched SDK/exporter dependencies with only the chosen OTLP transport features (e.g. HTTP protobuf via reqwest). Add validated optional endpoint and service-name configuration. Keep telemetry disabled when absent. Prefer trusted HTTPS collector endpoints in deployed environments and secret-backed authentication if needed.
2. Build tracer and meter providers with bounded batches/queues, exporter timeout (3 seconds), resource attributes, and an explicit sampling policy (e.g. parent-based 10% in production, always-on in tests). Compose the OpenTelemetry tracing layer with JSON formatting; replace the single `fmt().init()` in `main`. Return an owned guard/provider set so shutdown can flush it exactly once.
3. Extend `request_context` with request count, duration histogram, in-flight gauge and error categories. Label by matched route template, method from a finite allowlist, and status class; never by UUID, user/tenant, raw path/query or note title. Unmatched paths use one constant. Keep request IDs in logs/spans, not metric labels. Add dependency spans around `create_note` and pool acquisition without SQL text or bind values.
4. Propagate W3C trace context only according to the trusted-proxy model. Treat an inbound trace as correlation, never authorization. Outbound clients from [communication](communication.md) inject trace context only to intended downstream services; avoid leaking baggage. Continue returning the server's independent request ID.
5. Flush providers during graceful shutdown with a bounded total budget included in the orchestrator grace period. Exporter failure must drop/buffer bounded telemetry and report a safe counter, not fail requests or block shutdown forever. Collector reachability is not a web readiness dependency.

## Failure and verification

After implementing, run `cargo test --locked --test telemetry`, core checks and container smoke. Use an in-memory exporter for deterministic span/metric assertions and a local collector with a debug exporter for integration. Verify known route/status labels, request correlation, sampling, outbound parentage, no secret marker in serialized logs/spans, a bounded queue when the collector stalls, collector outage without HTTP failure, and shutdown flush. Add those tests and an opt-in collector service to CI. No hosted account is required until testing a vendor exporter.

Operate with request error rate, p95/p99 duration, saturation, pool acquisition failures and queue oldest age. Define alerts against actual SLOs, not every individual 404. Restrict collector access and retention; logs/traces may still contain identifiers that require policy review. Rollback can disable export while keeping JSON logs. Check cardinality and volume before increasing sampling. Record actual SDK versions and evidence when promoting this recipe.
