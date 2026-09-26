# Architecture implementation acceptance

Failure scenarios recorded before implementation, 2026-09-27.

- Two feature registries accidentally receive independent anonymous/write/concurrency budgets; changing a verifier accidentally resets those budgets or accepts another issuer.
- Disabling rate middleware produces an invalid OpenAPI response; bearer-only teaching routes advertise anonymous access; inspection and runtime disagree about enabled routes.
- A default build cannot recognize a database migrated by a batteries build; upgrading a released default database loses rows; repeated migrations or rollback mis-handle shared scheduling objects.
- Generated operations cannot share a transaction with a job; invalid input bypasses validation through the application interface; an ownership predicate is lost during extraction; a second failed write leaves a partially committed resource.
- Injected configuration is ignored in favor of process environment; constructing routes starts background tasks or panics on storage setup; shutdown leaves identity/stream tasks running; repeated mail ticks rebuild providers.
- Removing the teaching feature breaks readiness despite required application migrations being present.
- An inbox-only delivery consumer requires SMTP/HTTP/realtime dependencies; channel gates break the default compatible package surface.
- Export copies dirty/unrelated source while recording an immutable revision; exported manifests or migration SQL differ from their source revision; export failure leaves a misleading destination.

Verify through public framework/application interfaces, generated applications, isolated PostgreSQL schemas, configuration/feature combinations and exporter fixtures. Record executed commands and results separately from planned checks. Process E2E scripts retain their existing report artifacts.

## Executed focused checks

2026-09-27, Rust 1.98.1 in Ubuntu WSL, isolated PostgreSQL 16 on port 55439. The final combined repository gate uses the repository's PostgreSQL 18 container separately.

- The registry regression failed before implementation when rate limiting was disabled. After the fix, both registry tests passed, including shared concurrency across registries with different verifier selections.
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` passed after integration.
- `cargo test -p bracel-starter --test architecture` passed in default, no-default-features and all-features builds: three scenarios per build. These exercise shared quotas across notes/accounts, a released default migration history that skipped 000004 while retaining existing rows, repeat migration and rollback, lifecycle drain, and injected configuration in route inspection.
- `cargo check -p bracel-delivery --no-default-features` and each independent `mail`, `webhooks`, `realtime` configuration passed. The minimal dependency tree contains no lettre, reqwest, bracel-realtime or bracel-data.
- `py scripts/test_export_starter.py` passed all five fixtures, including immutable export from a dirty checkout and rejection of an unavailable revision. Both new cases failed before the exporter change.
- `bash scripts/generator-smoke.sh` passed. Evidence: `.scratch/generated-e2e/56e1b2328b6e4338b72858c424b0530d/report.json`, alongside command logs, source hashes and generated OpenAPI. The independently renamed application passed strict Clippy, all tests, ownership/cursor checks, and resource-plus-job commit/rollback scenarios for both generated resources. The smoke resource is Widget because the stable reference history already owns the projects teaching table.
- `bash scripts/api-e2e.sh` passed all 13 real-process scenarios: mutations, concurrency, realtime, WebSockets, delivery, outbound requests, files, collections, jobs, tenants, operations, middleware and identity. The completed log is `.scratch/architecture-evidence/bracel-architecture-api.log`; individual reports are under `.scratch/api-e2e/`.
- `git diff --check` passed.

The records above are focused verification of this change. They do not claim the later combined package, container and standalone export checks have finished. Those results belong in the coordinated verification record after all concurrent feature work stabilizes.

## Compatibility and adoption

- Existing core constructors and default delivery features remain available.
- The application keeps one known migration history in every build. Minimal runtime builds also install historical reference-package tables; remove teaching schema only when tailoring a fresh, unused application, or use new migrations for deployed data.
- Production startup calls fallible resource construction and then pure router assembly. The older `app` convenience remains for tests and embedded callers.
- Provider settings are parsed once by normal process startup; the account mail worker retains an explicit compatibility `from_env`/`mail_once` interface.
- The exporter now requires an available local commit and ignores dirty source. A new local commit is required to export these changes. No remote publishing is implied.
