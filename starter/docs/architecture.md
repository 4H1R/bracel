# Backend organization

The application is a modular monolith in one crate, depending on the Bracel framework for common HTTP, query and identity behavior. The [stack decision](adr/0001-stack.md) selects the runtime; [feature organization and HTTP contracts](adr/0002-feature-modules-and-http-contracts.md) records the design trade-offs.

`src/features/<feature>/mod.rs` defines the feature's public interface. The notes feature exports application operations and their input/output types; its SeaORM entity and implementation modules remain private. Other features call those operations, passing an existing transaction when composing atomic work. Cross-feature writes belong in an explicit coordinating operation, not in unrelated handlers.

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/lib.rs` | Process startup/CLI and shared application state; root re-exports `app` and `ApiDoc`. |
| `src/config.rs`, `src/db.rs` | Validated configuration and pool construction. |
| `src/middleware.rs` | Editable list of enabled common middleware; route authentication stays explicit. |
| `src/cache.rs` | Validated cache settings; AppState owns the shared runtime cache. |
| `src/jobs.rs`, `src/schedules.rs` | Job handlers and calendar definitions, available without example packages. |
| `src/cli/` | Application commands and operator diagnostics. |
| `src/http/mod.rs` | Router assembly, health/readiness and OpenAPI. |
| `src/batteries/` | Optional runnable examples for package integrations, including their commands and collection routes. |
| `bracel::http::middleware` | Common HTTP stack and explicit shared route policies; [composition guide](middleware.md). |
| `bracel::identity` | Bearer access-token verification and typed principal. |
| `bracel::query`, `bracel::http::query` | Allowlisted collection query application and bounded URL decoding. |
| `src/features/notes/query.rs` | Notes filter/sort declarations, also consumed by OpenAPI and inspect. |
| `bracel::http::error` | Shared application errors, validation issues and Problem rendering. |
| `bracel::http::response` | Typed `Data<T>`, `Page<T>` and `PageInfo` success contracts. |
| `bracel::http::pagination` | Limit validation and bounded, versioned cursor encoding/decoding. |
| `src/features/notes/http.rs` | Decode requests, map extractor errors, invoke operations, wrap responses. No database queries. |
| `src/features/notes/dto.rs` | Typed inputs/output and field-validation aggregation. Persistence fields are explicitly mapped to public output. |
| `src/features/notes/application.rs` | Concrete create/get/list operations, title rules, cursor position validation and notes query ordering. |
| `src/features/notes/entity.rs` | Private SeaORM persistence model. |
| `src/migrations/` | One ordered migration history across all features. Append rather than rewrite applied history. |
| `tests/support/mod.rs` | Shared isolated PostgreSQL fixture and HTTP contract helpers. |

Application operations accept dependencies explicitly and return typed results, not Axum responses. This small starter intentionally shares `AppError` and pagination input primitives with HTTP; it does not pretend to have a transport-independent domain layer. HTTP wrappers remain in handlers. Introduce feature-specific business errors and map them to HTTP when another transport or richer business logic requires that separation.

`bootstrap.rs` constructs provider resources and owns background-task shutdown. `provider_settings.rs` parses provider configuration through the same lookup as application configuration. `http::router(state, config, resources)` only assembles routes. The `app(state, config)` convenience used by tests also constructs resources; production startup uses the explicit fallible bootstrap path so invalid storage configuration is reported as a startup error.

`http::Registrations` composes each feature once for runtime routing, configured OpenAPI and inspection, sharing request budgets across groups. `ApiDoc` deliberately exposes an all-capabilities catalog; use `Registrations::configured(&config).openapi()` for the configured contract. Disabled routes remain marked with `x-enabled=false` for inspection.

The optional showcase uses private `batteries/projects.rs`, `batteries/files.rs`, `batteries/realtime.rs` and `batteries/collections.rs` modules. Provider settings enter through explicit extensions. No router reads process environment, starts a task or creates a directory.

Generated CRUD follows the same ownership pattern: its entity and application implementation are private, while `create_record`, `get_record`, `update_record`, `patch_record`, `delete_record` and `list_records` form the public interface. Writes validate typed inputs and enforce scopes and ownership. Ordinary operations accept `ConnectionTrait`; PATCH requires a caller-owned `DatabaseTransaction` to preserve its row lock. Test fixtures live in generated tests.

`src/features/accounts/` owns local users and account lifecycle: `application.rs` handles password/session transactions, `http.rs` registers the JSON routes, `dto.rs` defines safe public shapes, `settings.rs` validates account configuration, `auth.rs` resolves the current local user, and `mail.rs` delivers reset intents. It reuses Bracel bearer credentials and SMTP while keeping user policy and migration history application-owned. See [accounts](accounts.md).

Use DDD selectively: model invariants and use domain terminology when an operation has meaningful rules beyond CRUD. Create `domain.rs` only when those rules need it. Use concrete SeaORM operations rather than generic repository wrappers; external integration adapters can justify a small interface. A new feature needs only the files it uses, not an empty copy of every layer.

To extend the starter, use the resource generator or put feature behavior behind its module interface, register annotated handlers through Registry, append any migration, and verify success/failure behavior through application operations and real HTTP/database tests. Routing, schemas and inspection metadata follow the same registration. The [batteries guide](batteries.md) describes the generator and extension interfaces. Follow [data](features/data.md) for collection queries and [identity](features/identity.md) before adding private resources.
