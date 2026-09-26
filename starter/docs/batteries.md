# Building with Bracel 0.2

The framework supplies route registration, validated extractors, record policies, test clients, command registration and optional PostgreSQL jobs/tokens. Application code owns domain operations, state, schema history and deployment. The starter enables jobs and tokens; external integrations are opt-in Cargo features.

## Generate a resource

Install the matching CLI from the framework repository, then run inside a starter application:

~~~bash
bracel make resource Project --field name:string --field active:bool --field count:i64 --crud --dry-run --json
bracel make resource Project --field name:string --field active:bool --field count:i64 --crud
cargo fmt
bash scripts/dev.sh migrate
bash scripts/openapi.sh write
~~~

The generator writes an editable feature module, an appended migration and a PostgreSQL HTTP test. It registers the module and migration at explicit marker comments. Do not remove these markers while using generation. The command refuses existing output files, changed registration files, symlinks and invalid identifiers; dry run writes nothing and returns a versioned JSON plan containing before/after text. It never executes migrations or deletes records.

Names are ASCII identifiers; Project becomes the projects module/table/path. The initial field types are required string, i64 and bool. Strings trim whitespace and accept 1–200 Unicode characters. PUT replaces all declared fields; PATCH, relationships, nullable fields and custom pluralization require application edits. Request DTOs exclude id, owner and created_at. Response DTOs exclude owner. Ordinary Rust remains the source after generation.

Generated routes require projects:read or projects:write. Configure AUTH_MODE=bearer before calling them. All list/read/update/delete database operations include the issuer/subject owner key; a different owner receives 404 or an empty list. User filters cannot replace that predicate. A route scope grants access to the operation; it does not grant access to another owner's rows. The generated fixture supplies editable test input. Run the generated test with a disposable TEST_DATABASE_URL.

## Routes, requests and inspection

Each annotated handler is registered with Registry::register(routes!(handler), policy, enabled, query_parameters). The same registration creates the Axum route, OpenAPI schemas and security/rate responses, and the inspection entry. Applications may merge ordinary Axum routers, but those unregistered routes cannot appear automatically in inspection.

RoutePolicy::Scope fails closed without authentication. Public applies quotas; Exempt is for probes. Example is restricted to explicitly enabled teaching routes. Use the existing QuerySpec to supply the registered query parameters.

Implement Validate for a transport input with a typed Output, then accept ValidatedJson<Input> or ValidatedQuery<Input> in a handler. TypedPath<Uuid> maps path decoding to safe common errors. Fields rejects duplicate JSON keys, aggregates type/required/length issues and rejects unknown fields. Business validation remains in application operations when non-HTTP callers also need it.

~~~bash
cargo run -- inspect --json
cargo run -- commands
~~~

Inspection is read-only, includes schemas, filters, enabled routes, access requirements and command descriptions, and omits environment values. Its schema_version remains 1 with additive fields. Generator errors exit 2; normal application diagnostics preserve existing exit codes.

## Testing and authorization

Enable Bracel's testing feature only in dev-dependencies. TestDatabase::connect creates a random PostgreSQL schema; run your own Migrator against its db and call cleanup after success. Failed tests retain their schema for diagnosis. TestClient drives the actual router without opening a socket, supports bearer tokens, and provides status/validation assertions. It does not bypass authentication. testing::unique creates independent fixture names.

authorization::Policy<R> accepts a principal, action and record. authorize maps denial to 403. For private collections use authorization::owned on the base SeaORM Select, and include the same owner predicate in writes as the generator does. Tenant membership remains an application decision: validate membership before selecting a tenant and apply tenant predicates to every operation. The framework does not infer membership from a supplied tenant ID.

## Commands, jobs and schedules

Register application commands in src/commands.rs with a name, summary, argument names and an asynchronous handler. Commands share the initialized database and return JSON results. The current command list is available offline through commands.

~~~bash
cargo run -- db:seed
cargo run -- jobs:once
cargo run -- jobs:work
cargo run -- jobs:failed
cargo run -- jobs:retry JOB_UUID 'dependency repaired'
cargo run -- schedule:tick
cargo run -- schedule:work
~~~

The seed inserts one deduplicated example.ping job; the shipped worker implements that payload. Register real handlers explicitly by kind and payload version. There is no generic execution of arbitrary commands from job payloads.

Pass a business transaction to jobs::enqueue and commit it with your domain writes. A repeated dedupe key returns the original job only if kind, version, payload and attempt budget match. Failed transactions retain neither business changes nor their delivery intent. Payloads are limited to 64 KiB and should contain references rather than secrets.

Workers claim with SKIP LOCKED, increment attempts and fence completion/failure with a fresh lease token. The standard worker runs one job at a time with a 25-second deadline and 30-second lease; run more processes for parallelism. Retry backoff is persisted and capped, with jitter. Permanent and exhausted failures remain inspectable; replay requires an explicit reason and preserves the dedupe key. Shutdown stops new claims and finishes the bounded current attempt.

Delivery is at least once. A timeout or expired lease can duplicate an external effect; handlers must use provider idempotency or reconciliation. Lease fencing protects queue state, not a remote provider. There is no heartbeat for jobs exceeding the bounded deadline.

jobs::schedule explicitly creates or updates a named interval schedule. Updates preserve next_due_at. The scheduler uses database time, coalesces downtime into one job and advances the schedule atomically with enqueueing. Replicas share row locks. enable_schedule pauses/resumes it. Intervals are seconds; calendar cron, timezone/DST rules and no-overlap execution are not implemented. Schedules preserve their configured retry budget.

The starter's third migration installs queue, replay, schedule and token tables. Schema installation remains an explicit migration, never a side effect of constructing the framework.

## Keys and machine tokens

AUTH_PUBLIC_KEYS_JSON accepts a JSON object mapping kid to RSA public PEM. Use it instead of AUTH_PUBLIC_KEY_PEM. Key-set mode requires a matching JWT kid; unknown/missing identifiers fail closed. BearerAuth::replace_keys validates a replacement set and atomically updates clones, enabling an application-controlled reload. Keep old and new keys during rotation overlap. Token-provided key URLs are never fetched; automatic remote JWKS discovery is not included.

To accept opaque machine tokens alongside JWTs, set AUTH_MACHINE_TOKENS=true and migrate first. The administrator commands are (export the intended environment first; cargo run does not source .env):

~~~bash
cargo run -- tokens:issue YOUR_ISSUER MACHINE_SUBJECT 'projects:read projects:write' 3600
cargo run -- tokens:revoke TOKEN_UUID
~~~

Issuance prints the random secret once to stdout; treat that output as a credential. PostgreSQL stores only its SHA-256 digest with principal, scopes and expiry. Revocation and expiry are checked on every request. These commands are trusted operator tools; no public token issuance endpoint is installed.

## Optional integrations

Enable only what the application uses, for example cargo build --features mail,storage. The starter re-exports enabled modules as bracel_starter::integrations.

| Feature | Interface | Local development / tests |
| --- | --- | --- |
| mail | Mailer::relay uses TLS; message builds plain text and escaped HTML | Mailer::capture is bounded; Mailer::local connects only to loopback SMTP |
| storage | Storage wraps private S3, local files or memory; operations take a verified scope and UUID | Storage::memory / Storage::local; size and operation deadlines enforced |
| cache | ScopedCache bounds bytes and TTL, coalesces loads, and invalidates by scope/key | In-process Moka; no shared invalidation or global quotas |
| outbound | Outbound allowlists configured origins and bounds response bytes/time | Explicit loopback HTTP supports local servers; redirects and automatic retries disabled |
| telemetry | Telemetry::otlp creates a bounded OTLP trace exporter; tracer provides explicit instrumentation | Local OTLP collector; call shutdown to flush within a bounded budget |

Initialize adapters once and inject clones through application state or job handlers. Cache and storage scopes must come from verified identity/membership, never a client-supplied owner. Private S3 credentials/bucket policy remain deployment configuration. Local filesystem storage is for development or explicitly managed persistent disks, not an ephemeral container filesystem.

Outbound is for administrator-configured services, not arbitrary user URL fetching; it does not solve DNS rebinding for a general-purpose fetch proxy. Mail acceptance does not prove delivery, and ambiguous SMTP failures may duplicate messages. Telemetry must not contain bodies, tokens, raw URLs or personal data. Its export is optional and must not gate readiness.

See the [verification record](verification.md) for observed checks. Cloud credentials, hosted SMTP and production collector behavior require deployment-specific validation.

See [optional API packages](api-packages.md) for the subsequent API-only workflows and package split.
