# Framework and starter architecture review

Reviewed 2026-09-27 against the local working trees.

Implementation follow-up: shared budgets, stable migration registration, transaction-friendly generation, typed provider settings, lifecycle ownership, shared route composition, showcase modules, optional delivery channels and immutable exports are implemented locally. See [acceptance evidence](architecture-acceptance.md). The observations below record the review baseline. Multiple generated starter profiles and a different realtime ordering scheme remain optional future design choices.

## Recommendation

Keep the explicit Axum application, SeaORM/PostgreSQL, and modular monolith. The next investment should make composition consistent: shared request budgets, stable migrations, transaction-friendly generated features, and one place that constructs runtime dependencies.

The strongest existing module is the notes feature: callers use a small interface, its entity is private, and operations can participate in a caller-owned transaction. Apply that design consistently to generated code and the optional examples.

## Scope and confidence

- Framework checkout: HEAD `2bd1d0d`, with substantial uncommitted changes.
- Standalone starter: HEAD `08563ac`, also with uncommitted changes, pinning framework revision `2bd1d0dac95777a9d8ec36f8d4ed8e529c4cf9f6`.
- Inspected manifests, architecture decisions, framework interfaces, HTTP registration and policies, starter startup/configuration, accounts, notes, optional workflows, migrations, generators, tests and distribution scripts.
- Compared reference and standalone `src/` and `tests/`, ignoring CRLF/LF differences: 21 reference files are different or absent in the standalone tree. This includes work in progress, so it is not evidence that the published release is broken.
- This is a source review. No Rust, database, container or exporter tests were run. Cargo, rustc and Python were not found on the current native PATH. `wsl --list --quiet` returned `Wsl/EnumerateDistros/Service/E_ACCESSDENIED`.
- Runtime consequences below are source-derived risks unless explicitly described as an observed code property. Existing verification records concern earlier runs and do not verify the current working tree.

## What to preserve

1. **Application-owned state and migrations.** Framework construction does not implicitly connect to a database or migrate it.
2. **Feature interfaces with private persistence.** Notes exposes operations and DTOs, using `ConnectionTrait` to compose transactions without a generic repository layer.
3. **Explicit route policies.** Authentication, scopes and enabled routes are visible in registration.
4. **Transactional intent.** Jobs, events, audit and idempotency can share business transactions.
5. **Public-interface verification.** Package consumer checks, generated application checks and process E2E scripts already provide a useful foundation.
6. **Opt-in provider integrations.** Keeping provider dependencies outside the default core is the right direction.

## Prioritized improvements

### 1. Share request budgets across feature registries

**Priority: high. Scope: framework and starter.**

`Registry::new` creates `Policies::new(config)`. Each `Policies` owns new rate limiters and a concurrency semaphore. The starter constructs separate registries for its main routes, accounts and optional batteries, then merges their routers. Consequently those groups have separate budgets even when configured with the same global-looking settings. The framework guide explicitly asks applications to share one policy instance.

Accounts also installs its own verifier. Simply forcing every group to use the same complete `Policies` object would lose that distinction.

**Change:** separate shared request-budget state from authentication selection. Construct budgets once at application assembly; allow each route group to choose a verifier and access policy while sharing those budgets. Prefer explicit constructors or builders over a global container. Preserve the existing constructor as a compatibility convenience if needed.

**Acceptance:** configure an anonymous quota of one, call routes in two different feature groups from the same peer, and verify the second request is limited. Verify aggregate concurrency across groups and retain tests for local-account versus external-token authentication.

Evidence: [registry](../crates/bracel/src/http/registry.rs), [policy state](../crates/bracel/src/http/middleware/mod.rs), [application assembly](../starter/src/http/mod.rs), [account router](../starter/src/features/accounts/http.rs).

### 2. Make migration history independent of runtime build features

**Priority: high. Scope: starter and release checks.**

Migration `000004_api_packages` is conditionally registered with `cfg(feature = "batteries")`. The same application therefore has different known histories in different builds. The new `000006_job_scheduling` repeats schema installation from `000004` and checks whether `000004` is recorded before undoing it. That is concrete evidence that schema ownership already overlaps.

**Change:** give each deployed application a stable migration history. Keep released migrations intact. Use either a fixed migration target containing the required schema dependencies, or application-owned frozen SQL snapshots that do not require optional runtime packages. For new starter profiles, choose the initial migrations when generating the application; after adoption, changes are append-only. Document how existing installations transition before changing registration.

**Acceptance:** test a released default database upgraded to the current default build, a released batteries database upgraded to the current batteries build, and feature changes against an already migrated database. Verify existing rows, migration records and rollback ownership. The precise current failure on feature removal needs runtime reproduction.

Evidence: [migration registry](../starter/src/migrations/mod.rs), [optional schema](../starter/src/migrations/m20260926_000004_api_packages.rs), [scheduling migration](../starter/src/migrations/m20260927_000006_job_scheduling.rs).

### 3. Generate the architecture that the starter teaches

**Priority: high. Scope: CLI templates.**

The notes feature keeps its entity private and has operations accepting a connection or transaction. The resource template emits a public entity and HTTP handlers that directly query `state.db`; PATCH opens and commits its own transaction. An adopter who wants to create a resource and enqueue a job atomically must first extract the generated HTTP logic.

**Change:** generate private persistence plus application operations, typed inputs/outputs and thin HTTP handlers. Operations should accept the database dependency explicitly, with transaction ownership visible at the coordinating operation. This can use private inline modules or separate files; the interface matters more than the directory count. Keep current ownership predicates and scoped access defaults. Move fixture construction into test support.

**Acceptance:** extend the generated-application smoke test with a generated resource write plus job intent in one transaction. Prove both commit together and both roll back when the second write fails. Retain cross-owner denial and cursor tests.

Evidence: [resource template](../crates/bracel-cli/templates/resource.rs), [generator](../crates/bracel-cli/src/generate.rs), [notes operations](../starter/src/features/notes/application.rs), [notes interface](../starter/src/features/notes/mod.rs).

### 4. Give startup, configuration and shutdown one owner

**Priority: medium-high. Scope: starter.**

Configuration is partly parsed through `Config::from_lookup`, but startup, commands, handlers and diagnostics also read environment variables independently. The batteries router creates directories, constructs storage and spawns a shutdown task. The account mail loop reconstructs settings and its mailer inside `mail_once`. Main spawns identity discovery tasks without retaining their join handles, and several places independently wait for shutdown signals.

This makes router construction effectful and prevents an injected configuration from fully determining runtime behavior. For example, diagnostics reports generic email configuration only from `MAIL_LOCAL_PORT`, while account settings support an SMTP relay.

**Change:** add an application-owned bootstrap module. Parse typed settings once, construct providers once, and pass dependencies into routers and workers. Let a runtime owner retain background task handles, broadcast one shutdown signal and await tasks within a bounded drain period. Keep settings near their feature; bootstrap composes them. Provider construction failures should return startup errors rather than panic while assembling a router.

**Acceptance:** construct two applications with different supplied settings without changing process environment; verify diagnostics matches those settings. Exercise startup failure, an in-flight request, an active stream and a background worker during shutdown. Prove provider handles are reused by repeated mail ticks.

Evidence: [startup](../starter/src/main.rs), [batteries router](../starter/src/batteries/mod.rs), [account mail](../starter/src/features/accounts/mail.rs), [diagnostics](../starter/src/cli/tooling.rs).

### 5. Use the same route description for serving, inspection and OpenAPI

**Priority: medium-high. Scope: framework and starter.**

Registry is a good starting point, but the application separately assembles runtime routers, `ApiDoc`, and diagnostic inventory. Account runtime registration replaces the verifier, while documentation uses a default HTTP config. The batteries route macro manually constructs schemas and status codes. `RoutePolicy::Example` emits an anonymous security alternative even when runtime bearer mode requires a scope.

There is also a concrete edge case in the current registry: disabling rate limiting skips construction of its 429 response, but the code still inserts `Retry-After` into `responses["429"]`. That can create an incomplete response object. Existing middleware-removal tests construct raw routers, so they do not exercise this registration path.

**Change:** assemble explicit route descriptions once, carrying availability, access policy, typed contract and handler registration. Render runtime routes, configured inspection and documentation from those descriptions. If an all-capabilities OpenAPI catalog is useful, expose it as a deliberate separate mode. Use typed OpenAPI builders internally where practical and return registration errors instead of panicking on generated JSON.

**Acceptance:** compare declared routes/policies with served routes under accounts on/off, bearer on/off, batteries on/off and rate limiting on/off. Validate the resulting OpenAPI document, including error response descriptions. Preserve deliberate differences between a catalog and a deployment's enabled contract.

Evidence: [registry](../crates/bracel/src/http/registry.rs), [HTTP assembly and ApiDoc](../starter/src/http/mod.rs), [inventory](../starter/src/cli/tooling.rs), [batteries route macro](../starter/src/batteries/mod.rs), [middleware tests](../crates/bracel/tests/middleware_options.rs).

### 6. Make optional examples easy to remove

**Priority: medium. Scope: starter packaging.**

The reference application now serves as an account starter, a notes teaching slice and an integration showcase. The batteries module contains project mutations, files, notifications, events, webhooks and metrics. Its Cargo feature enables several packages together. Readiness still checks the notes table even when teaching routes are disabled.

**Change:** first split the showcase by workflow behind private feature interfaces. Then offer a small API profile, an accounts profile and a showcase profile if adopter demand warrants multiple generated variants. Keep the reference showcase in the framework workspace for integration tests. Make readiness reflect the adopted application's required schema, so removing notes does not leave an unrelated deployment dependency.

**Acceptance:** generate an application without teaching features, run its documented checks, and verify its migration list, route inventory and readiness contain only its selected capabilities.

Evidence: [batteries](../starter/src/batteries/mod.rs), [starter manifest](../starter/Cargo.toml), [readiness](../starter/src/http/mod.rs), [adoption instructions](../starter/README.md).

### 7. Narrow package coupling where dependencies are incidental

**Priority: medium, after composition fixes. Scope: optional framework crates.**

`bracel-delivery` enables both mail and HTTP integrations and depends on realtime. Both delivery and files depend on `bracel-data`; its generic `sql` and `conflict` helpers are used outside the data workflows. Thus an apparently small helper can pull an unrelated package into the dependency graph. Jobs already demonstrates a package that works without depending on the HTTP core.

**Change:** gate independently usable delivery channels, and keep generic SQL/error conveniences local where they are not meaningful public contracts. Within existing crates, separate persistence behavior from HTTP response mapping when workers or other callers benefit. Split into additional crates only when independent consumers justify them. Use workspace dependency declarations to keep shared versions/features deliberate.

**Acceptance:** compile and inspect dependencies for each documented consumer configuration outside the workspace. An inbox-only consumer should not acquire SMTP and outbound HTTP solely because of delivery's manifest. Preserve intentional dependencies, such as publishing notification events when that behavior is enabled.

Evidence: [delivery manifest](../crates/bracel-delivery/Cargo.toml), [files manifest](../crates/bracel-files/Cargo.toml), [data helpers](../crates/bracel-data/src/lib.rs), [jobs manifest](../crates/bracel-jobs/Cargo.toml).

### 8. Verify the exported application as a release artifact

**Priority: medium-high. Scope: distribution and CI.**

The exporter pins an input revision but copies the current `starter/` tree. It validates SHA syntax, not that the copied source belongs to that revision. With the current dirty working tree, it can therefore export newer starter code against an older framework. That is a reproducibility risk, not proof that a release has already done so.

Workspace checks test all features together; the standalone check tests defaults. These are useful but do not establish that the same exported artifact supports each intended configuration or upgrades a previous database correctly.

**Change:** export from an immutable selected checkout/archive. Record source revision, framework revision and content hashes. Use a small supported feature matrix covering defaults, no default features, batteries and meaningful independent integrations. Build the export outside the workspace without path patches to the current framework, and run previous-release database upgrades there. Treat the standalone repository as generated release output; preserve local work until it has been reconciled deliberately.

**Acceptance:** reject or clearly identify an export whose source differs from its recorded revision. Re-exporting the same revision should produce the same application files after documented generated metadata/lockfile handling. Run generated CRUD composition and migration-transition scenarios against that artifact.

Evidence: [exporter](../scripts/export-starter.py), [workspace checks](../scripts/check.sh), [starter checks](../starter/scripts/check.sh), [distribution](distribution.md).

## Proposed composition

```text
Application bootstrap
  -> validated feature settings
  -> database + providers + shared request budgets
  -> feature registration -> router / OpenAPI / inspection
  -> workers + schedules + one shutdown owner

Feature HTTP handlers
  -> feature application operations
  -> private persistence and explicit framework workflows

Migration target
  -> fixed application-owned history
```

These are ownership roles, not a requirement for a new crate or trait at every arrow.

## Suggested delivery order

1. Specify failing public-interface scenarios for cross-feature budgets, registry contracts and migration feature transitions; fix those contracts.
2. Make generated CRUD match the notes interface and prove transaction composition.
3. Consolidate bootstrap/configuration/shutdown and derive inspection from the same declarations.
4. Simplify the starter showcase, narrow optional dependencies, and strengthen immutable export verification.

Each step should preserve existing user changes and released migration history. No implementation, version bump or release action was performed by this review.

## Scaling question to measure later

Realtime publication updates one `bracel_event_clock` row and holds its lock until the business transaction commits. This intentionally gives resume cursors commit-safe ordering, but serializes event-publishing transactions across scopes. Measure throughput and lock wait under representative concurrent writes. If it becomes a bottleneck, evaluate per-scope ordering or an outbox publisher with an explicit cursor migration. Retain the current correctness guarantee until an alternative is proven; there is no measured performance result in this review.

Evidence: [event publication](../crates/bracel-realtime/src/lib.rs).
