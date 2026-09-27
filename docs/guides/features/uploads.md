# Uploads and private object storage

Implementation update (2026-09-26): consult [the optional API package guide](../api-packages.md) for the current implemented workflows, package boundaries and limits. The design recipes below remain guidance for application-specific extensions; they are not verification evidence.

Bracel 0.2 provides the optional storage adapter using object_store 0.14.2, with scoped opaque keys, bounded reads/writes, memory/local adapters and an S3 backend constructor. See [batteries](../batteries.md). The metadata, presigning, scanning and provider-specific deployment flow below remains an application recipe.

## Earlier recipe and further extensions

## When and choice

Use object storage when files must survive replica restarts, exceed small JSON payloads, or be downloaded independently. The core database is sufficient for notes; do not write persistent uploads into the container filesystem. Recommend private S3-compatible storage, presigned upload/download operations, and PostgreSQL metadata. Use AWS SDK for S3 for the AWS deployment path; confirm compatibility and credential semantics for another S3 provider.

Checked 2026-09-25: [aws-sdk-s3 1.149.0](https://docs.rs/aws-sdk-s3/1.149.0/aws_sdk_s3/) (`1`), [presigning](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/presigned-urls.html), [S3 policy conditions](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-HTTPPOSTConstructPolicy.html). Verify the SDK's MSRV against `rust-toolchain.toml` before adding it and lock a compatible SDK/config release together. **Documentation-only; SDK compilation and local object-store behavior have not been verified.**

## Prerequisites and edits

Implement [identity/authorization](identity.md) first for private files. Decide maximum size, allowed types, malware scanning, retention, object-store provider, and tenant quotas. Read [jobs](jobs.md) for scan/cleanup tasks. Edit `Cargo.toml`, `src/config.rs`, `src/lib.rs::AppState` and `src/http/mod.rs::app`, `src/main.rs::run`, and `src/migrations/mod.rs::Migrator`. Create `src/features/files/mod.rs`, `src/features/files/entity.rs`, `src/storage.rs`, `tests/files.rs`, and a local-only object-store Compose override. `storage.rs` is a concrete integration boundary; a small trait is justified if tests need a controlled failure implementation.

## Implement

1. Add SDK dependencies and config for region, bucket, endpoint, and allowed maximum object size. Use the default credential chain in production; restrict test credentials to the local override. Build one client in `run`, with explicit connect (2s) and operation (10s) timeouts and bounded SDK retries. Add it to state. Do not use SDK defaults without reviewing them.
2. Migrate upload metadata: UUID id, owner/tenant, random storage key, expected size/type/hash, status (pending/quarantined/available/deleted), and expiry. A client filename is display metadata, not a storage path. Add an authorized initialize route that checks quota and inserts pending metadata.
3. Return a short-lived upload capability (e.g. 5 minutes) scoped to one random key. Use a presigned POST policy when enforcing content-length-range at upload; a presigned PUT alone must not be described as a complete size limit. Require signed headers where supported and validate actual stored metadata after upload. Keep bucket listing/public access off.
4. Add an idempotent complete route. Authorize the caller, HEAD the object, verify size/checksum and allowed content, then mark it quarantined or available. Never trust a browser's reported MIME or size. If scanning is required, enqueue a scan job and serve only after success. Attach a file to a note with a foreign key and the same owner/tenant check.
5. Authorize each download before signing a short-lived GET. Treat the URL as a bearer secret, return safe Content-Disposition, and avoid active user HTML on the application's origin. Logical deletion revokes future downloads and enqueues physical deletion; previously signed URLs may remain valid until expiry. Cleanup expires abandoned pending uploads in batches and aborts incomplete multipart uploads.

## Failure, verification, and operations

Database and object store do not share a transaction. Model pending/failed states explicitly and make completion/cleanup retryable. A missing object must not produce a database row claiming availability. SDK retries can duplicate requests; use stable object keys and idempotent operations. Set quotas before issuing URLs and reconcile abandoned reservations. Authentication in the initialize route does not replace authorization in complete/download/delete.

After creating tests, run `cargo test --locked --test files` against an isolated local S3-compatible service and PostgreSQL, then core/container checks. Cases: cross-user/tenant access, forged key, oversized file, type mismatch, checksum failure, expired URL, duplicate completion, absent object, database failure after upload, scan rejection, cleanup race, and provider outage. Test presigned policies in the selected provider's sandbox before claiming production compatibility; local emulators may differ. CI adds the local service and requires its URL.

Deploy scoped IAM permissions, TLS, encryption and bucket lifecycle rules. Monitor upload latency/failure, pending age, quarantine, storage growth and cleanup backlog. Object-store outage should disable file operations while notes/health remain available unless the product cannot function without files. Rollback keeps metadata state readers compatible; never delete live objects to undo an application release. Include storage objects and metadata consistency in recovery drills.
