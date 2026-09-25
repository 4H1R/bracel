# Backend organization

The application is a modular monolith in one crate, depending on the Bracel framework for common HTTP, query and identity behavior. The [stack decision](adr/0001-stack.md) selects the runtime; [feature organization and HTTP contracts](adr/0002-feature-modules-and-http-contracts.md) records the design trade-offs.

`src/features/<feature>/mod.rs` defines the feature's public interface. The notes feature exports application operations and their input/output types; its SeaORM entity and implementation modules remain private. Other features call those operations, passing an existing transaction when composing atomic work. Cross-feature writes belong in an explicit coordinating operation, not in unrelated handlers.

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/lib.rs` | Process startup/CLI and shared application state; root re-exports `app` and `ApiDoc`. |
| `src/config.rs`, `src/db.rs` | Validated configuration and pool construction. |
| `src/http/mod.rs` | Router assembly, health/readiness and OpenAPI. |
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

Use DDD selectively: model invariants and use domain terminology when an operation has meaningful rules beyond CRUD. Create `domain.rs` only when those rules need it. Use concrete SeaORM operations rather than generic repository wrappers; external integration adapters can justify a small interface. A new feature needs only the files it uses, not an empty copy of every layer.

To extend the starter, put feature behavior behind its module interface, merge its routes in `http::app`, add its paths/DTOs to `ApiDoc`, append any migration, and verify success/failure behavior through application operations and real HTTP/database tests. Update `tooling::inventory` for new capability statuses or route gates. Follow [data](features/data.md) for collection queries and [identity](features/identity.md) before adding private resources.
