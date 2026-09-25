# Email rendering and delivery

## When and choice

Use email for transactional messages after a product needs them. The core has no mail dependency or provider setting. Recommend Askama for compile-time HTML templates, Lettre for SMTP, and Mailpit for local capture. A synchronous send can serve a low-volume explicit test/admin action; durable user-facing delivery requires [jobs](jobs.md), and note-triggered delivery requires an atomic note + intent transaction.

Checked 2026-09-25: [Askama 0.16.1](https://docs.rs/askama/0.16.1/askama/) (`0.16`), [Lettre 0.11.23](https://docs.rs/lettre/0.11.23/lettre/) (`0.11` with defaults off and `builder,smtp-transport,tokio1-rustls-tls`), [Mailpit 1.31.2](https://github.com/axllent/mailpit/releases/tag/v1.31.2). Versioned local trial assets live in `docs/features/email-example/`, not the application. **Verification: disposable integration executed; see [the dated record](../verification.md).** That covers rendering, escaping, MIME creation, local SMTP capture and connection failure only. Production TLS/auth/provider feedback and durable dispatch remain documentation-only.

## Prerequisites and edit map

Decide synchronous versus durable delivery, approved sender, recipient source, template data, and provider. Follow [identity](identity.md) for private recipient ownership and [jobs](jobs.md) for reliable dispatch. Edit `Cargo.toml`, `src/config.rs::Config`, `src/lib.rs::AppState`, `src/main.rs::run`, `Dockerfile`, and `.dockerignore`; create `src/email.rs`, `templates/note.html`, and `tests/email.rs`. The three corresponding `.txt` files in `email-example/` are the bounded local trial, with deliberately loopback-only plaintext transport. A real integration should inject an initialized mailer in state, not reconstruct it per request.

## Implement the local slice

1. In a disposable copy, add the dependencies above, export `pub mod email;` from `src/lib.rs`, and copy the trial files to the new paths. The commands and copies are executable in `scripts/exercise-email.sh`. It preserves the core manifest/lockfile and leaves the copy in `.scratch/email-<timestamp>-<pid>` for review. When adopting the integration, allow `templates` and `templates/**` in `.dockerignore` and add `COPY templates ./templates` before the Dockerfile's Cargo build. Askama needs the templates at compile time; no runtime template volume is needed. Re-run container smoke after this adoption step (the local trial does not build an email-enabled image).
2. Start only a local capture service: `docker run --rm -p 127.0.0.1:1025:1025 -p 127.0.0.1:8025:8025 axllent/mailpit:v1.31.2`. The exercise script allocates its own container on ports 11025/18025 and cleans it on exit. The Mailpit API is [documented here](https://mailpit.axllent.org/docs/api-v1/).
3. Render a typed title to an HTML template with automatic escaping and a plain-text alternative. Construct addresses with Lettre's mailbox parser, not header string concatenation. Return typed safe errors; never expose transport errors, addresses, subjects, or bodies in logs.
4. For actual application use, replace the trial's `local_mailer` with validated config: sender, SMTP host/port, TLS mode and secret references. Permit plaintext only for explicit loopback development. Production must use Lettre's TLS relay builder and credentials, plus a 5-second transport/outer deadline. Add a mailer handle to `AppState`; initialize once in `run`. Route code should call the email service or enqueue an intent using the note ID, not duplicate rendering/transport logic.
5. For durable mail, replace the trial's static-string errors with a safe enum distinguishing temporary SMTP/network failure, ambiguous timeout, permanent rejection, and invalid input; classify the Lettre error before discarding its sensitive text. Implement a versioned `SendNoteNotification` job. Insert the intent with the note transaction in `create_note`; have the worker load the authorized recipient and render/send after commit. Do not automatically email arbitrary addresses supplied to the unauthenticated example route. Decide whether content is an immutable snapshot or the latest note revision. Store minimum necessary data.

## Failures and verification

SMTP acknowledgement means accepted by the relay, not delivered to the inbox. Retry network/4xx failures through jobs; treat invalid addresses and permanent 5xx as failed, with bounded retries and a replay workflow. A timeout after relay acceptance is ambiguous and can lead to duplicate email. Use a stable Message-ID for correlation, but do not claim it provides deduplication. Escape untrusted template data; never mark note content safe HTML.

Run `bash scripts/exercise-email.sh` from the finished starter (Rust, Docker, Python 3, curl required). It creates a copy, installs the optional dependencies there, runs formatting/Clippy and `cargo test --locked --test email`, and reads the Mailpit API to assert exactly one captured message with the intended local recipient and escaped body. Review the printed copy path/lockfile. The script sends only to a local capture service, with no real external email.

When implemented for a project, move the relevant checks into normal CI, add local capture as an opt-in CI service, and run all core checks in that configuration. Add acceptance tests for invalid addresses, escaping/Unicode, connection refused, a relay that stalls, 4xx vs 5xx classification, credentials/TLS enforcement, duplicate jobs, and note/intent rollback. Use a provider sandbox only for provider-specific tests; require explicit sandbox setup and never use production recipients in CI.

## Operate

Configure SPF/DKIM/DMARC with the selected provider and domain owner. Track accepted/failed/bounced messages using opaque delivery IDs. Authenticate provider feedback webhooks before updating suppression status; deduplicate event IDs and account for out-of-order events. Enforce suppression/preferences before dispatch. [Communication](communication.md) covers notification routing. Web readiness should not depend on an external SMTP provider when delivery is queued; monitor worker backlog and oldest delivery age instead.

Retain only necessary delivery metadata and redact addresses. Rotate SMTP credentials independently of template changes. Roll back code with payload-compatible workers; pause delivery if a template is wrong, repair it, then replay selected failed jobs. Removing optional email means removing its state/config/dependencies and worker task explicitly, not leaving a required but unused provider setting.
