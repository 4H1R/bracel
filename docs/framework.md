# Framework interface

Bracel owns shared HTTP mechanics, registration, validation, commands and optional job/token infrastructure. An application owns configuration for its integrations, state, routes, business operations, migrations, OpenAPI and process lifecycle. There is no global container or automatic database migration.

~~~rust
use bracel::{
    Application, config::Config,
    axum::{Router, Json, routing::get},
    http::{middleware::{Policies, Access}, response::Data},
};

let config = Config::from_lookup(|_| None).unwrap();
let policies = Policies::new(&config);
let routes = policies.apply(
    Router::new().route("/hello", get(|| async { Json(Data::new("hello")) })),
    Access::Public,
);
let app = Application::new(config).merge(routes).build(());
~~~

Config contains HTTP settings only; constructing it does not require a database. Application accepts any cloneable, Send + Sync application state. Register all feature routers before calling build(state). The builder supplies common 404/405 responses, request IDs, logs, CORS, body limits and deadlines. Serve with peer connection metadata to give rate limits distinct client identities.

The starter embeds framework settings in its application config's `http` field. Use `config.http` when constructing Policies or reading HTTP settings; database and feature settings stay on the application config.

Create one Policies instance for all resource routers so their quotas and concurrency limit are shared. Apply Access::Scope("resource:read") to protected groups; a missing verifier fails closed. Access::Public still applies anonymous/write quotas. Health routes may remain outside policies. See [middleware](../starter/docs/middleware.md) for ordering, limits and custom Axum layers.

Handlers use Data, Page, AppError and ValidationErrors. The query module applies explicitly allowed filters and sort columns to an existing SeaORM select, preserving mandatory predicates. Feature-owned declarations drive parsing and query parameter documentation. Cursors bind filters, order and access scope, but never replace authorization.

The [notes module](../starter/src/features/notes/mod.rs) is the complete database example. It remains application code; use DDD when business invariants warrant it, without generic repository wrappers for ordinary CRUD.

The library re-exports Axum, SeaORM and utoipa so consumers can share compatible types. The starter keeps convenient re-exports for its own handlers. Registry generates runtime routes, OpenAPI and route inspection from handler registration. Application-specific doctor and migration checks remain in the starter. See [the batteries guide](../starter/docs/batteries.md) for generation, policies, test helpers, commands, jobs, key rotation and integrations.
