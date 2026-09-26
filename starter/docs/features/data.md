# Data access extensions

Implementation update (2026-09-26): consult [the optional API package guide](../api-packages.md) for the current implemented workflows, package boundaries and limits. The design recipes below remain guidance for application-specific extensions; they are not verification evidence.

Notes cursor pagination, shared query filtering/sorting and database fixtures are implemented as of 2026-09-25. Seeds, optimistic concurrency and search below remain recipes. Sources: [SeaORM queries](https://www.sea-ql.org/SeaORM/docs/basic-crud/select/), [transactions](https://www.sea-ql.org/SeaORM/docs/advanced-query/transaction/), [PostgreSQL text search](https://www.postgresql.org/docs/18/textsearch.html), [Spatie's allowlist design](https://spatie.be/docs/laravel-query-builder/v7/introduction).

## Pagination and filtering

`GET /example/notes?limit=25&after=...` uses `ListNotes` and `list_notes` from the notes feature. The default limit is 25 and allowed range is 1–100. It fetches limit + 1, returns at most limit rows ordered by `(created_at DESC, id DESC)`, and emits the cursor from the last returned row only when there is another row. The response is `Page<Note>` with `data` and `page: {limit, has_more, next_cursor}`. Empty and final pages have `has_more: false` and `next_cursor: null`; no count query runs. Pass the cursor unchanged to request the next page; page size may change between requests.

The second migration adds non-null `created_at timestamptz` and `notes_created_at_id_idx (created_at DESC, id DESC)`. Existing rows share the migration statement timestamp because their original creation time is unknown; new rows use the database clock. Dates serialize as RFC 3339 UTC with database microsecond precision. ID is a tie-breaker only: mixed legacy UUID v4 and UUID v7 rows remain supported. Pagination compares the tuple with `<`, not an offset. The notes operations never modify creation time.

Shared `bracel::http::pagination` handles limits and bounded base64url cursors; feature logic defines the typed position. The shared query parser computes a scope from resource version, caller scope, canonical filter values and effective sort using SHA-256. Cursors carry envelope version 1, that scope, and a timestamp/UUID position. Old cursors from before filtering was introduced are intentionally incompatible. Empty, malformed, over-1024-byte, incompatible or invalid-position tokens return 422 with an `after` validation issue. Out-of-range numeric limits return 422; malformed, duplicate, overflowed or unsupported query parameters return safe 400 Problem Details. Notes rejects sub-microsecond and out-of-range cursor timestamps before querying the database.

Cursors are opaque to clients but are neither signed nor encrypted. The scope hash binds navigation context; it is not authentication. In bearer mode notes binds cursors to the verified issuer/subject; in local anonymous mode it uses a public scope. The teaching collection is shared among callers with the required notes scope. Before adding owned/tenant collections, constrain every query by the current principal/tenant and include that tenant context in the query scope. A valid or forged cursor must never widen access. If a product needs tamper-proof claims, select a signed or server-stored cursor design explicitly.

Traversal is not a frozen snapshot: rows created ahead of the cursor do not shift subsequent pages, deleted anchors remain usable, and rows inserted behind the cursor may appear later. Backdated inserts, creation-time changes through external SQL, or changing filter membership can change results across requests. Exports needing a fixed snapshot require a separate snapshot/export workflow. Offset pagination remains appropriate for small admin screens needing page numbers after documenting count costs.

The shared `bracel::query` applies explicit column allowlists to a feature's existing SeaORM Select, preserving its mandatory WHERE constraints and replacing its ORDER BY with the declared stable order. `bracel::http::query` bounds URL queries to 4096 bytes and 16 fields and rejects duplicate/invalid encoding. Notes declares filters and sorts once in `src/features/notes/query.rs`; these declarations also generate OpenAPI parameters and the inspect inventory.

| Parameter | Behavior |
| --- | --- |
| filter[id] | Exact UUID |
| filter[title] | Case-sensitive literal substring, including literal %, _ and backslash |
| filter[title_exact] | Exact case-sensitive text |
| filter[created_after] | Inclusive lower timestamp bound |
| filter[created_before] | Inclusive upper timestamp bound |
| sort | created_at or -created_at; default -created_at, with ID in the same direction |

Filters combine with AND. Values must be nonempty and at most 200 Unicode characters. Dates accept RFC3339 at microsecond precision and normalize timezone offsets. Invalid values return 422; unknown names/operators return 400 without echoing submitted values. Multi-value lists, OR groups, arbitrary sort expressions, relationship includes and sparse fields are not supported. The substring filter uses parameterized LIKE with escaped wildcards; it is not full-text search and may scan rows. No new text index is claimed. The existing timestamp/ID index supports both ascending and descending traversal.

```text
/example/notes?filter[title]=meeting&sort=created_at&limit=25
/example/notes?filter[title]=meeting&sort=created_at&limit=25&after=RETURNED_CURSOR
```

To add a collection, declare `QuerySpec` with filters mapped to typed SeaORM columns and complete sort keys. Parse the bounded fields with the verified access scope; validate the feature's cursor position; apply the resulting `CollectionQuery` to an authorized base query, add the matching keyset predicate and fetch limit+1. End every sort with a unique tie-breaker and initially allow only immutable non-null columns. A new sort needs a matching typed cursor and index, not just a new public name. Return the existing Page DTO and expose the spec's parameters through OpenAPI. Cursor scope must change when collection semantics change. Writes remain feature operations.

`tests/pagination.rs` exercises timestamp ties, mixed UUID versions, default/max limits, page boundaries, deleted anchors, newer inserts, malformed cursors, validation before database work, prior-schema upgrades and the index definition. `tests/query.rs` adds literal wildcard/SQL-shaped values, exact/date filters, ascending pages, changed-filter/sort rejection and canonical timestamp scope. See [verification](../verification.md) for executed checks and query-plan evidence.

## Transactions and fixtures

Use a SeaORM transaction in the concrete application function for multi-row changes; pass the transaction to entity operations. Keep external calls outside transactions. Verify rollback by deliberately violating a constraint on a later write and asserting the earlier write is absent. For concurrent edits, add a version column and conditional update to reject stale versions with a safe 409; test concurrent writers.

`tests/support/mod.rs` now supplies isolated schema setup, the HTTP response-contract helper and a direct persistence assertion shared by HTTP and pagination tests. Add deterministic fixture builders there as features need them; no SQLite substitute. An optional `src/bin/seed.rs` can create clearly named demonstration data via an explicit command and idempotent keys. Require an explicit local seed flag; production startup never seeds, truncates, or recreates the database. Test running the seed twice.

## Search

Start with PostgreSQL full-text search if ranking/tokenized text is sufficient. Add a `tsvector` column or expression index with an explicit language configuration and a GIN index in a new migration. Query with parameterized `websearch_to_tsquery`, combine tenant/ownership filtering before returning results, and bound query length and page size. Test language/stemming, punctuation, Unicode, empty query, ranking ties, authorization and query-plan/index use on representative data. External search engines are a decision only after requirements for typo tolerance, facets, scale or language support justify synchronization/outbox work.
