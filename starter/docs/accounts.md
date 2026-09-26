# Ready-to-use API accounts

The starter owns `src/features/accounts/` and the `users` migration. Bracel supplies bearer verification, hashed token storage, HTTP policies and the optional SMTP adapter. No application user table or route is added to the framework library.

Copy `.env.example`, run `bash scripts/dev.sh up`, `bash scripts/dev.sh migrate`, then `bash scripts/dev.sh run`. The example environment selects `AUTH_MODE=local`, enables accounts, and disables teaching routes. PostgreSQL and local Mailpit start through Compose. In a second terminal run `bash scripts/dev.sh auth:mail-work` to deliver password resets. Inspect local mail at `http://127.0.0.1:8025`; this is developer tooling, not an application frontend.

```bash
curl -s http://127.0.0.1:3000/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.test","display_name":"Your name","password":"a long unique password"}'
# Read data.access_token from the response; pass it as Authorization: Bearer TOKEN.
curl -s http://127.0.0.1:3000/api/users/me -H "Authorization: Bearer $TOKEN"
```

| Method and path | JSON input | Result |
| --- | --- | --- |
| `POST /api/auth/register` | `email`, `display_name`, `password` | 201, `data.user`, `data.access_token`, `token_type`, `expires_in` |
| `POST /api/auth/login` | `email`, `password` | 200, same session response |
| `GET /api/users/me` | Bearer credential | 200, current user's ID, email and display name |
| `PATCH /api/users/me` | Bearer credential and `display_name` | 200, updated profile |
| `POST /api/auth/logout` | Bearer credential | 204, current session revoked |
| `POST /api/auth/logout-all` | Bearer credential | 204, all this account's sessions revoked |
| `POST /api/auth/forgot-password` | `email` | 202, identical acknowledgement for existing and missing accounts |
| `POST /api/auth/reset-password` | `token`, `password` | 204, replace password and revoke every account session |

Responses use `Data<T>` and shared problem details; account responses carry `Cache-Control: no-store`. Unknown and duplicate JSON fields are rejected. Email addresses are trimmed and ASCII-normalized to lowercase with database uniqueness. Passwords are never trimmed; accept 15–128 Unicode characters, with a 512-byte bound. Argon2id uses the RustCrypto 0.5 defaults (19 MiB, two iterations, one lane), random salts, and a four-operation blocking-work limit per router. Unknown-user login performs equivalent password-hashing work and returns the same invalid-credentials detail. Database errors and hashes never enter public DTOs or logs.

Sessions are 256-bit random opaque bearer credentials, hashed in `bracel_tokens`, and linked to users in `account_sessions`. They expire after one day. Logout is checked against PostgreSQL on subsequent requests across replicas. Login and reset serialize on the user row so a login verified against an old password cannot issue a token after reset. There is no refresh credential: log in again after expiration. Send credentials only over HTTPS outside local development.

`AUTH_MODE=local` makes these tokens available to normal Bracel scope-protected routes. Issued users receive only `account:self`. Add application permissions deliberately in `accounts/application.rs::session`, and use the verified principal plus owner/tenant predicates in each feature. Registering does not grant administrative, example-note, or optional package scopes. The account handlers additionally require the local issuer and a matching user; an external issuer's matching subject cannot access them. `AUTH_MODE=bearer` continues to select external JWTs for other resources; account endpoints independently accept local sessions. `AUTH_MODE=off` preserves anonymous teaching mode, while account profile/logout endpoints still require authentication. Cookies are not accepted.

`ENABLE_ACCOUNTS=false` removes account routes (also select a non-local AUTH_MODE); retain the migration history. `ALLOW_REGISTRATION=false` closes registration while keeping existing users able to log in. There is no public user directory, arbitrary user update/delete, email-change workflow, email verification, MFA, social login or breached-password lookup. A registered email has **not** been proven to belong to the caller. Add verified-email gates before using email ownership as authorization.

## Reset delivery and operations

Mail is enabled in the starter's default Cargo features; the framework adapter remains optional. `--no-default-features` retains accounts/login but requires enabling `mail` for resets. Without configured mail, forgot-password returns the same 503 for every address.

Local delivery uses `MAIL_LOCAL_PORT=1025` and `MAIL_FROM`. Production removes `MAIL_LOCAL_PORT` and sets `MAIL_SMTP_HOST`, `MAIL_SMTP_USERNAME`, `MAIL_SMTP_PASSWORD`, and an approved `MAIL_FROM`. The relay uses TLS with certificate validation; plaintext delivery is limited to loopback. Configure provider/domain SPF, DKIM and DMARC separately.

Forgot-password commits a reset intent in `account_resets`, without a raw token. `auth:mail-work` claims pending intents using PostgreSQL leases and SKIP LOCKED, generates a secret in memory, persists its hash, and sends a plain-text email with an HTML alternative. The message contains a token for the client's JSON reset flow; no HTML reset page or client-supplied redirect URL exists. A token expires 15 minutes after its request and can be consumed once. A new request supersedes earlier requests. Each delivery retry generates a fresh token and invalidates the previous attempt; in an ambiguous SMTP failure an earlier email may contain a superseded token. Only the latest delivered token works. Five attempts, a 30-second retry interval, and a 30-second lease bound delivery; permanent SMTP rejection stops retries. Relay acceptance is not proof of inbox delivery.

Run `auth:mail-once` to process one item. Monitor pending/expired reset counts, attempts and worker health using database/operations tooling; these commands never print message bodies or credentials. After a delivery problem is fixed, the user can request another reset. Run `auth:cleanup` periodically; each invocation removes up to 1000 expired reset intents, quota buckets and expired/revoked account credentials per table. Quotas count attempts per normalized-email hash across replicas: 10 logins, 5 registrations and 3 forgot requests per 15-minute window. Existing HTTP peer/write quotas also apply; use edge limits for distributed IP abuse controls. Registration returns 409 for a duplicate email and therefore discloses availability; forgot/login use generic outcomes. No hard account lockout is installed.

## Verification

`bash scripts/accounts-e2e.sh` requires disposable `TEST_DATABASE_URL` and runs the real binary, migrations, HTTP clients, multiple replicas and a local SMTP peer. It retains a repeatable JSON report, redacted transcript, logs and source/binary/lock hashes under `.scratch/accounts-e2e/<run>/`. The root and standalone check scripts run it. [Acceptance cases](accounts-acceptance.md) were recorded before implementation; [verification](verification.md) records executed results.

Design references: [RustCrypto Argon2](https://docs.rs/argon2/0.5.3/argon2/), [OWASP password storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html), [OWASP password resets](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html).
