# Bracel integrations

Optional adapters for mail, owner-scoped object storage, caching, bounded outbound
HTTP and OTLP traces. No adapters are enabled by default. Select the mail, storage,
cache, http or telemetry Cargo features.

Applications initialize adapters once and inject them into state or job handlers.
Errors expose safe categories rather than provider messages. Local capture,
memory, filesystem and loopback HTTP adapters support tests without cloud credentials.

See the framework's starter/docs/batteries.md for interfaces, examples and limits.
