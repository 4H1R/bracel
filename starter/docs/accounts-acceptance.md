# Local account acceptance contract

Written before implementation. Drive the real server, PostgreSQL migrations, and a local SMTP peer; retain a redacted JSON report and transcript with binary/source hashes.

- Registration validates email, display name and a 15–128 character password; rejects unknown/duplicate fields, short passwords, and duplicate normalized emails. Concurrent duplicate registration creates one user. Never serialize password hashes.
- Login accepts normalized email, rejects incorrect passwords and unknown accounts with the same 401, issues bounded-expiry random bearer credentials stored only as hashes, and applies shared database email quotas plus HTTP peer quotas. Bound concurrent password hashing off the async executor.
- Only the current account can read/update its profile. No public user directory or caller-selected roles/scopes. Local accounts cannot be impersonated by an external issuer using the same subject.
- Logout revokes the presented session; logout-all and password reset revoke all account sessions. Expired/revoked tokens fail closed across replicas and restarts.
- Forgot-password returns the same accepted response for known and unknown addresses. Queue intent durably without storing a raw reset credential. Deliver through configured SMTP; provider outage retries with bounded attempts. Missing mail configuration returns a uniform 503.
- Reset credentials are random, hashed, expire, and are consumed once atomically with password replacement and session revocation. Invalid, expired, replayed and concurrent reset requests cannot change the account. An in-flight old-password login cannot create a session after reset commits.
- Worker retries generate a fresh secret and supersede earlier attempts. Stale requests cannot overwrite newer requests. Cleanup bounds retained expired resets, sessions and quota records.
- JSON responses retain shared envelopes/problem details, no-store headers, OpenAPI and inspect registration. Logs/artifacts must not contain passwords, bearer credentials, reset secrets or SMTP credentials.
- Existing external bearer authentication, anonymous teaching routes, optional packages, generated applications and standalone starter export continue to work.
