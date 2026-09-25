# HTTP middleware and route policies

Implemented 2026-09-25. Middleware lives in `bracel::http::middleware`; bearer verification lives in `bracel::identity`. Use Axum/Tower composition, with one shared `Policies` instance for application route groups. `src/features/notes/http.rs` demonstrates separate read and write groups. See [verification](verification.md) for executed checks.

## Common stack

Request order is request context → CORS → deadline → JSON body limit → route policy → handler. Responses unwind in reverse. The context creates a UUID v7, adds typed `RequestContext` to request extensions, normalizes errors and adds `X-Request-ID` and `X-Content-Type-Options: nosniff`. Logs contain only the route template, request ID, status and elapsed time. Never enable raw request/header/body logging when adding a layer.

The deadline is inside CORS so timeout responses retain allowed-origin headers. CORS preflight runs before authentication and quotas, but inside request logging. Error normalization preserves `WWW-Authenticate`, `Retry-After`, and CORS headers. Middleware returns `AppError` or a response containing it; success DTO wrappers stay in handlers. The deadline covers producing the response and reading JSON, not the lifetime of future streaming responses. JSON body limits do not replace streaming byte limits for raw-body/upload routes.

`CORS_ORIGINS` is a comma-separated list of at most 16 exact HTTP(S) origins, without trailing slashes, paths or wildcards. Empty means no allowed browser origins. Allowed methods are GET, HEAD, POST, PUT, PATCH, DELETE and OPTIONS; allowed request headers are Authorization and Content-Type. Exposed response headers are X-Request-ID and Retry-After. Credentials are not enabled. A disallowed origin receives no CORS permission; CORS is not authentication and does not reject non-browser clients.

## Applying policies

Construct `Policies::new(&config)` once at router assembly and share its clones. Register routes before calling `apply`. For example, in an adopting feature:

```rust,ignore
let reads = Router::new().route("/documents", get(list_documents));
let writes = Router::new().route("/documents", post(create_document));
let routes = policies.apply(reads, Access::Scope("documents:read"))
    .merge(policies.apply(writes, Access::Scope("documents:write")));
```

`Access::Public` is explicit anonymous access; `Access::Scope` requires verified identity and the named scope, and fails with 503 if no verifier was configured. Public routes do not attempt optional authentication or consume a supplied bearer token. Notes deliberately chooses Public when AUTH_MODE=off for local teaching; use explicit Scope policies for real private resources. Merge feature routers before applying the common stack. Health/readiness routes have common middleware but no application rate or concurrency policy.

For another middleware, use `axum::middleware::from_fn` (or `from_fn_with_state`) and attach it with `route_layer` to the completed feature router before `Policies::apply` if it needs verified identity. A feature-specific guard stays beside that feature; reusable HTTP behavior belongs under `bracel::http::middleware`. Attach application layers before `Application::build`; extend the framework for changes to its common stack, keeping request context outermost and CORS outside layers that can return authentication/rate/deadline errors. Update route policy metadata in `src/http/contracts.rs` and inventory gating when registering a new feature, then regenerate OpenAPI and test the assembled stack.

A route policy applies the shared anonymous quota using socket peer IP, acquires a concurrency permit without queueing, verifies authentication/scope when required, applies the authenticated quota by `(issuer, subject)`, then applies the write quota for unsafe methods. Handlers receive `Extension<Principal>`; pass verified identity into application operations when enforcing ownership. The HTTP notes example is a shared collection: `notes:read` permits reading all notes and `notes:write` permits creating notes. It does not implement per-user ownership or tenants. Follow [identity](features/identity.md) before adding those semantics. Middleware cannot protect calls from workers or other non-HTTP callers; those operations need their own explicit authorization input.

## Local limits

| Setting | Default | Meaning |
| --- | --- | --- |
| RATE_ANONYMOUS_PER_MINUTE | 120 | Shared pre-authentication allowance per peer IP, across policy-protected routes |
| RATE_AUTHENTICATED_PER_MINUTE | 60 | Allowance per verified issuer/subject, across protected routes |
| RATE_WRITES_PER_MINUTE | 20 | Additional unsafe-method allowance per principal, or anonymous peer |
| RATE_MAX_KEYS | 10000 | Maximum keys in each of the three quota maps |
| MAX_IN_FLIGHT | 64 | Maximum simultaneous handler/auth operations across policy groups |

Governor supplies replenishing quotas: each bucket initially allows a burst equal to its per-minute allowance and refills continuously. Rejected requests return 429 with rounded-up `Retry-After` seconds. Quota storage is bounded; new keys at capacity return 503 with Retry-After. Idle keys are evicted during a sweep at most once a minute, after at least a minute idle. Restarting clears quotas. Capacity exhaustion never silently admits traffic. Concurrency exhaustion returns 503/Retry-After: 1 and queues no work; dropping/cancelling the handler releases its permit.

The server installs Axum `ConnectInfo<SocketAddr>`. Tests or embedders without peer metadata share an `unknown-peer` bucket. Forwarded and X-Forwarded-For are ignored, so clients cannot select a quota key. Behind a reverse proxy, anonymous limits aggregate on that proxy's address; size them accordingly and implement per-client limits at the edge. Trusted-proxy parsing is not implemented. All quotas are per process; replicas multiply the allowance. A shared gateway/store and explicit outage policy are needed for global quotas. Probe exemptions do not replace edge connection/header limits.

## Authentication and CSRF

Set AUTH_MODE=bearer and configure AUTH_PUBLIC_KEY_PEM, AUTH_ISSUER and AUTH_AUDIENCE. See [bearer authentication](features/identity.md#implemented-bearer-access-tokens). No cookie, session, query-string token or browser login is accepted. CSRF protection is therefore not part of this bearer-only contract. Introducing cookie authentication requires Origin/CSRF enforcement on unsafe methods before exposing it; a CORS allowlist alone is insufficient.

`inspect --json` reports runtime authentication requirements, scope, named api/writes rate policy and query parameters per route. The generated OpenAPI document covers both configuration modes and marks bearer security optional only because explicit local AUTH_MODE=off exists. Its extensions document the required scope/rate policy. Unknown configuration yields unknown authentication, not evidence of public access.

Sources used: [Axum middleware ordering](https://docs.rs/axum/0.8.9/axum/middleware/), [Governor quotas](https://docs.rs/governor/0.10.4/governor/_guide/), [Tower HTTP CORS](https://docs.rs/tower-http/0.7.1/tower_http/cors/). Custom middleware must be tested through the assembled router, including early rejection, CORS, error rendering, secret redaction, deadline cancellation and resource release.
