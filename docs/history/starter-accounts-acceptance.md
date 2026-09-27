# Local account acceptance contract

Written before implementation. Drive the real server, PostgreSQL migrations, and a local SMTP peer; retain a redacted JSON report and transcript with binary/source hashes.

## Default accounts and verification extension (2026-09-27)

Record these cases before changing the implementation:

- Without AUTH_MODE, local account sessions protect application routes. Explicit off and bearer modes retain their behavior; disabling accounts requires an explicit alternative mode.
- Readiness succeeds without the notes table when examples are disabled, fails when an enabled account table/column is missing, and checks notes when examples are enabled.
- Missing, null, wrongly typed and invalid account fields produce aggregated, safe field issues; unknown fields use the root path and duplicate fields remain rejected. Validation never echoes input secrets.
- A long-running mail worker reuses its initialized configuration and transport across multiple deliveries and retries.
- Registration reports email_verified=false and queues verification atomically when mail is configured. Upgrading existing users never marks their email verified.
- Authenticated resend queues only the current user's stored email, with shared quotas. Confirmation requires that same user, the same email and a hashed, unexpired token; reset and verification tokens are not interchangeable.
- Verification tokens are consumed once under concurrent requests; expired, superseded, wrong-user, wrong-email and replayed tokens fail. A successful verification is immediately visible across replicas without changing scopes or revoking sessions.
- Missing mail returns 503 for resend, anonymous/external identities cannot request or confirm verification, and verified users cannot queue more mail. Delivery retries rotate secrets and cleanup removes expired intents.
- Migration upgrades preserve existing accounts. OpenAPI and inspection describe verification routes and the safe boolean field; evidence contains no raw secrets.

- Registration validates email, display name and a 15–128 character password; rejects unknown/duplicate fields, short passwords, and duplicate normalized emails. Concurrent duplicate registration creates one user. Never serialize password hashes.
- Login accepts normalized email, rejects incorrect passwords and unknown accounts with the same 401, issues bounded-expiry random bearer credentials stored only as hashes, and applies shared database email quotas plus HTTP peer quotas. Bound concurrent password hashing off the async executor.
- Only the current account can read/update its profile. No public user directory or caller-selected roles/scopes. Local accounts cannot be impersonated by an external issuer using the same subject.
- Logout revokes the presented session; logout-all and password reset revoke all account sessions. Expired/revoked tokens fail closed across replicas and restarts.
- Forgot-password returns the same accepted response for known and unknown addresses. Queue intent durably without storing a raw reset credential. Deliver through configured SMTP; provider outage retries with bounded attempts. Missing mail configuration returns a uniform 503.
- Reset credentials are random, hashed, expire, and are consumed once atomically with password replacement and session revocation. Invalid, expired, replayed and concurrent reset requests cannot change the account. An in-flight old-password login cannot create a session after reset commits.
- Worker retries generate a fresh secret and supersede earlier attempts. Stale requests cannot overwrite newer requests. Cleanup bounds retained expired resets, sessions and quota records.
- JSON responses retain shared envelopes/problem details, no-store headers, OpenAPI and inspect registration. Logs/artifacts must not contain passwords, bearer credentials, reset secrets or SMTP credentials.
- Existing external bearer authentication, anonymous teaching routes, optional packages, generated applications and standalone starter export continue to work.
