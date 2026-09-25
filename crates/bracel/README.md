# Bracel

An opinionated Axum and SeaORM framework with typed queries, cursor pagination,
HTTP response contracts, JWT verification and composable middleware.

Applications own their routes, state, database and migrations. Start with
Application, config::Config and http::middleware::Policies.

See the framework repository for the reference starter and interface guide:
https://github.com/4H1R/bracel


Use http::registry::Registry with utoipa_axum::routes to register runtime routes,
contracts and inspection together. http::extract provides validated inputs;
authorization provides record policies and owner filtering. Optional jobs,
tokens and testing features add PostgreSQL queues, machine credentials and
isolated test helpers. External adapters live in bracel-integrations.
