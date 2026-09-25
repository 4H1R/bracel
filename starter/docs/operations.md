# Configuration, deployment, and recovery

Bearer auth and middleware settings are listed in `.env.example` and [middleware](middleware.md). AUTH_MODE=off is for local teaching; use explicit protected route policies and AUTH_MODE=bearer for real private resources. Issuer public-key rotation currently requires configuration update/restart. Local rate budgets multiply across replicas and restart with the process; socket-peer quotas aggregate behind proxies. Tune RATE_* and MAX_IN_FLIGHT together with edge and database limits. No Redis or remote key discovery is required at startup.

`Config::from_env` reads only environment variables. `DATABASE_URL` is required; malformed values fail without echoing secrets. `BIND_ADDR` defaults to loopback, and `ENABLE_EXAMPLE` defaults false. Numeric bounds and defaults live in `src/config.rs`, `bracel::config::Config` and `.env.example`. No unused optional settings are required. Local `.env` is ignored by Git and excluded from the Docker context. Inject production secrets from the deployment's secret store into process environment; rotate credentials with rolling restarts, and use PostgreSQL TLS with certificate verification (`sslmode=verify-full` and trusted roots) outside trusted local networks. Never bake secrets into image layers or CI.

Build using `bash scripts/build-image.sh your-app:VERSION`. Run `your-app:VERSION migrate` once with the intended database URL before running `serve`; supply both as deployment environment, not Dockerfile values. The runtime is non-root and contains CA certificates but no shell tooling for probes beyond its base OS. Use the orchestrator's HTTP probes: `/healthz` for liveness, `/readyz` for readiness. Keep examples disabled. Terminate TLS at a trusted reverse proxy with public connection/header/body limits. Set CPU/memory/pool budgets: replicas × `DB_MAX_CONNECTIONS` plus workers and maintenance connections must fit the PostgreSQL limit.

Start with a 30-second termination grace period (20 in the smoke test with the default 10-second request deadline). Remove the instance from load balancing, send SIGTERM, allow draining, then enforce the platform's hard deadline. Long-lived streams require an explicit broadcast close signal; see [realtime](features/realtime.md). Application stdout is structured JSON. Monitor readiness, 5xx, deadline counts, and database capacity; [observability](features/observability.md) adds metrics and tracing without making an exporter a readiness dependency.

## Migration recovery

Use `bracel-starter inspect --database --json` to view applied/pending history without changing it, or `doctor --deploy --database --json` to fail on pending migrations and enabled teaching routes. These commands use the supplied process environment and do not check external deployment infrastructure. See [developer tooling](features/tooling.md).

Before releasing a schema change, test it from an empty database and from a fixture matching the previous release. Prefer expand/backfill/contract: add compatible columns/indexes, ship code accepting both states, backfill in bounded batches, then remove old storage in a later release. Set a deployment statement/lock timeout so a migration cannot wait indefinitely; inspect locks and retry deliberately. The serving request deadline does not bound the migration command. Serialize migration jobs in the deploy platform.

If migration fails, stop the rollout, inspect `seaql_migrations` and the actual schema using an administrator, and determine whether the failed step committed. Single SQL statements are atomic; a custom multi-step migration may need repair. Restore from a validated backup or apply a reviewed forward repair. Do not blindly delete migration-history rows or run `down` to make startup pass. Roll back application code only if it remains compatible with the current schema. The initial example's down migration deletes notes and is for test lifecycle coverage.

## Backup/restore drill

Choose RPO/RTO before production. Use managed PostgreSQL automated backups plus point-in-time recovery for most hosted projects. Logical backup is a portable additional path:

```bash
# Use a secret-backed PG service entry or PGPASSFILE; do not put passwords in shell history.
pg_dump --dbname=service=production --format=custom --file=backup.dump
createdb --maintenance-db=service=restore_admin restore_drill
pg_restore --dbname=service=restore_drill --no-owner --exit-on-error backup.dump
```

Define those service entries for the project's environment first. Restore into an isolated database, check migration history, verify table counts/constraints and representative application reads, and measure elapsed time against RTO. Do not point test teardown at the restored production service. Encrypt and restrict backup artifacts, set retention/deletion policy, and rehearse at least quarterly. These operator commands are guidance; no backup or production restore was performed during starter verification.
