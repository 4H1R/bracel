# Laravel Boost mechanics and a Bracel equivalent

Research checked 2026-09-27 against [Laravel 13 Boost documentation](https://laravel.com/framework/docs/13.x/boost) and Boost source at [`c0b668c`](https://github.com/laravel/boost/tree/c0b668cd2d5e565507b5bd05ee71f8e322c660e7). This is an implementation proposal, not a shipped Bracel capability. The source snapshot matters: Boost changes quickly, and its published docs can lag the current `main` command behavior.

## Recommendation

Build a Bracel development companion through `bracel ai`: a small set of generated instructions, task skills, version-matched documentation, and local inspection tools. Start with local search and reuse Bracel's existing inspection commands. A hosted semantic search service can come later.

The freshness contract should be **the newest knowledge that matches the code this application actually uses**. A consumer pinned to an older framework commit must keep that commit's API guidance. A developer editing Bracel itself should see the working tree, including uncommitted changes. Updating knowledge and upgrading the framework are separate actions.

This follow-up also inspected the local Bracel working tree based on `2bd1d0dac95777a9d8ec36f8d4ed8e529c4cf9f6`. That tree contains ongoing edits; local implementation observations below describe the inspected files, not a released artifact.

## How Boost works behind the scenes

Boost is a development dependency installed with Composer. `boost:install` asks which agents and three features to configure: guidelines, skills and an MCP server. It detects installed packages and their major versions, assembles matching guidance, then writes each selected agent's expected files. For Codex, those defaults are `AGENTS.md`, `.agents/skills` and `.codex/config.toml`. The latter launches the local server through `php artisan boost:mcp`; the command delegates to Laravel MCP's `mcp:start laravel-boost`. [Install command](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Console/InstallCommand.php), [Codex adapter](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/Agents/Codex.php), [MCP start command](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Console/StartCommand.php).

| Layer | Mechanism | Freshness boundary |
| --- | --- | --- |
| Guidelines | Core, conditional, installed-package and selected third-party files are rendered and composed into one agent instruction block. Project `.ai/guidelines` files add guidance or override matching paths. The writer replaces only the `<laravel-boost-guidelines>` block, preserving surrounding hand-written content. | Snapshot when `boost:install` or `boost:update` runs; already running agents may need a new session to reload files. [Composer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/GuidelineComposer.php), [writer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/GuidelineWriter.php). |
| Skills | Built-in, package and project `.ai/skills/{name}/SKILL.md` skills are selected from the installed package set. A project skill with the same name wins. The writer syncs agent-specific skill directories and removes previously tracked stale skills. Skills are loaded by the agent only when relevant. | Generated copies are snapshots. Custom Markdown-only skills use symlinks when possible, so source edits appear directly; copying is the fallback. Client reload behavior still matters. [Skill composer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/SkillComposer.php), [writer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/SkillWriter.php). |
| Project rules | App-specific decisions live in committed `.ai/rules` files, indexed by path globs. The `record-rule` tool updates the index; agents are told to consult matching rules before editing. An `infer-conventions` skill can identify existing patterns for review. | Rules change with the application repository, independent of framework releases. [Docs](https://laravel.com/framework/docs/13.x/boost#project-rules). |
| Live tools | The project's MCP process exposes application/package info, logs, database metadata, read-only query, documentation search and other selected tools. The server has include/exclude configuration. | Each tool executes in a fresh application subprocess. The persistent parent may still need restarting for server registration/configuration changes. [Server registry](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Boost.php), [executor](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/ToolExecutor.php). |
| Search Docs | A local MCP tool gathers installed PHP and JS package names and major versions, optionally filters packages, and posts queries plus that inventory to Laravel's hosted `/api/docs` endpoint. Laravel describes the hosted index as embedding-based semantic search. Bundled guidelines tell agents to use it for version-sensitive APIs. | Search is on demand and version-filtered. Its results depend on the hosted corpus being current; it does not inspect uncommitted local source. [Tool implementation](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/SearchDocs.php), [hosted API description](https://laravel.com/framework/docs/13.x/boost#documentation-api), [instruction example](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/.ai/boost/core.blade.php). |

Boost's local MCP server and its hosted documentation API have different jobs. `Application Info` reports the app's actual framework/package versions at call time; `Search Docs` uses those versions to retrieve relevant published material. Neither mechanism guarantees the agent has read the specific current code path: the agent still needs to inspect source and nearby tests for code changes. The Boost best-practices skill explicitly instructs that check. [Application info](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/ApplicationInfo.php), [Search Docs](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/SearchDocs.php), [best-practices skill](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/.ai/laravel/skill/laravel-best-practices/SKILL.md).

The invocation path is agent → MCP `tools/call` → Boost's executor → `php artisan boost:execute-tool` → tool result. The executor removes inherited variables defined by `.env` so the child can reload current values, and applies a timeout (180 seconds by default). This helps PHP code/environment freshness. Launching a fresh Rust binary would reload its environment but would still execute its previously compiled code, so Bracel needs build provenance as well. [Executor source](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/ToolExecutor.php).

Search sends package versions as `<major>.x` to `https://boost.laravel.com/api/docs`, alongside queries, a token limit and Markdown output format. That payload does not distinguish minor releases or a local working tree. Laravel documents semantic search with embeddings, but the inspected sources do not establish its embedding model, storage backend or indexing delay; those should not be presented as verified implementation details. [Search client](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/SearchDocs.php), [documentation API](https://laravel.com/framework/docs/13.x/boost#documentation-api).

### Update behavior and an observed documentation mismatch

`boost:update` validates saved `boost.json` configuration, then silently invokes the install command for the already enabled guidelines and skills. The Laravel docs suggest placing it in Composer's `post-update-cmd`. Current source discovers newly available third-party package guidance by default and prompts in an interactive terminal; `--no-discover` opts out. It deliberately skips the prompt inside Composer scripts and noninteractive runs, so a dependency update hook will refresh selected content but will **not automatically select guidance for a new third-party package**. The Laravel 13 page still describes `--discover` as opt-in; follow the behavior of the version actually installed. [Update source](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Console/UpdateCommand.php), [installation docs](https://laravel.com/framework/docs/13.x/boost#keeping-boost-resources-updated).

Third-party guidance is found under direct installed package `resources/boost/guidelines` and `resources/boost/skills`, with package selection recorded in configuration. This limits surprise instruction injection from transitive dependencies. First-party package/version guidance is selected through the package inventory; core, conditional and project guidance are merged. [Third-party discovery](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/ThirdPartyPackage.php), [package discovery](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/Concerns/DiscoverPackagePaths.php), [composer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/GuidelineComposer.php).

Discovery considers both PHP and JavaScript packages. Selection also resolves overlapping guidance, such as preferring Pest over PHPUnit and Flux Pro over Flux Free. `boost:update` regenerates resources available to the installed Boost package; upgrading Boost itself is a separate Composer operation, and neither command guarantees the hosted documentation index has caught up. [Selection rules](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/Concerns/DiscoverPackagePaths.php), [third-party inventory](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/ThirdPartyPackage.php), [update implementation](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Console/UpdateCommand.php).

The MCP tools are consequential capabilities, not just documentation access. For example, Boost's database query tool requests a database read-only transaction and rollback, while its Tinker code execution tool is disabled by default through `boost.tinker_tool_enabled`. A Bracel implementation should start with bounded, read-only inspection and require explicit enablement for any code execution or database mutation capability. [Database query tool](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/DatabaseQuery.php), [Tinker tool](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/Tinker.php).

## Bracel-specific design

Bracel has a different packaging boundary. The framework owns reusable mechanics; applications own state, routes, migrations and feature policy. The starter is a separate reference application, while framework dependencies are pinned by revision in exported starters. Consequently, a Bracel agent aid should ship **versioned framework guidance with framework releases**, while each consuming app keeps its own rules and current-state inspection. [Framework interface](../framework.md), [distribution](../distribution.md), [split decision](../adr/0003-bracel-framework.md).

Recommended components, in implementation order:

1. **Canonical knowledge in the framework repo.** Create concise, reviewed `ai/guidelines` and `ai/skills` sources alongside the relevant crates, plus a machine-readable capability catalog. Each feature entry should state status, owning crate/module, public API symbol, minimum/version or Git revision, dependencies, example, verification command and links to canonical docs/tests. Include *implemented, optional, recipe-only and planned* states so agents cannot turn plans into claims. Generate release guidance from this catalog and source docs, rather than hand-maintaining copies in the starter. The current [capability catalog](../../starter/docs/features/index.md), [framework guide](../framework.md) and [distribution contract](../distribution.md) are the starting inputs.
2. **Installer/sync command in `bracel-cli`.** A proposed `bracel ai install` detects the consumer's exact `bracel-*` dependency revision/version and chosen agents, then writes marked guidance blocks, agent skill files and per-project MCP configuration. A proposed `bracel ai sync` refreshes only generated material, preserves content outside marked blocks, and reports local overrides/conflicts. Keep `AGENTS.md` and app-owned rules reviewable in Git; record a generation manifest containing Bracel revision, source hashes and generator version. This is a design proposal modeled on Boost's [installer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Console/InstallCommand.php) and [marked-block writer](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Install/GuidelineWriter.php).
3. **Local read-only MCP server.** Start with `application_info` (resolved crate revisions/features, Rust/toolchain, configured adapters), `capabilities` (compiled/configured/verified), `routes` or OpenAPI inspection, `doctor`, and `search_docs`. Pull actual app information from its manifest, lockfile and runtime inspection rather than guessing from template docs. Keep DB schema/log tools opt-in and redact values; do not provide arbitrary SQL or code execution in the initial version. The CLI and starter already expose generation and inspection patterns to build on. [Bracel CLI](../../crates/bracel-cli/README.md), [framework interface](../framework.md), [Boost tool examples](https://laravel.com/framework/docs/13.x/boost#mcp-server).
4. **Version-aware documentation search.** For an initial local implementation, index Markdown headings and public API examples from the *installed framework revision* plus the consumer repo using lexical search, with result URLs, revision and line references. This directly covers unpublished/current Bracel code and works offline. A hosted semantic index can follow when there are enough released versions and published docs to justify it; key every chunk by repository, commit/tag, crate and feature status. Prefer exact version filters, then clearly label a nearest-version fallback. This is an inference from Boost's [installed-package filtering](https://github.com/laravel/boost/blob/c0b668cd2d5e565507b5bd05ee71f8e322c660e7/src/Mcp/Tools/SearchDocs.php) and Bracel's [revision-pinned distribution](../distribution.md).
5. **Task skills and app rules.** Ship task-focused skills for HTTP routes, validation/query contracts, identity, jobs, migrations and E2E verification. A short root guideline should tell the agent to identify the installed Bracel revision, search version-matched docs, inspect current code and tests, follow app-owned rules, and report evidence. Put app-specific policy and architecture in committed local rules with glob matching. This follows Boost's [guideline/skill/rule split](https://laravel.com/framework/docs/13.x/boost#guidelines-vs-skills) without copying Laravel-specific instructions.

### Keeping it current when every feature changes

Treat feature completion as a change to code **and** the canonical capability entry, public documentation, example, and focused E2E evidence. Add a CI gate that compares public crate/module/CLI/capability changes against catalog/docs updates; this is a drift detector, not an automatic claim that prose is correct. Run generation deterministically in CI and fail if generated agent files differ. Test that a fixture consumer pinned to an older revision receives older guidance, and a current consumer receives the new feature. The generated manifest should make staleness visible to `bracel ai doctor` and to the agent at chat start. These are proposed controls, not present behavior.

After a framework dependency update, run `bracel ai sync` from the package/update workflow, refresh the local search index, and start a new agent session when its startup instructions or skills have changed. For an uncommitted framework checkout, let the MCP inspection tool report its dirty revision and read local files directly; do not present an old hosted index as the latest truth. A feature is available to a consumer only when its resolved dependency revision, compiled features and configured services support it. This follows Bracel's [distribution rules](../distribution.md) and Boost's distinction between [generated resources](https://laravel.com/framework/docs/13.x/boost#keeping-boost-resources-updated) and [on-demand versioned search](https://laravel.com/framework/docs/13.x/boost#documentation-api).

The first useful milestone is therefore a reviewed capability manifest, one concise generated agent block, two task skills, a version-aware local `search_docs` tool, and a `doctor` freshness check exercised in two fixture consumers at different framework revisions. It delivers a reliable freshness chain before adding a hosted embeddings service or high-privilege tools.

## Concrete implementation plan

Everything in this section is proposed unless explicitly identified as existing.

### Reuse the current application contract

The existing [starter tooling implementation](../../starter/src/cli/tooling.rs) emits `schema_version: 1` JSON, route registrations, OpenAPI schemas, command metadata, and capability information. Its `doctor` command reports stable check codes. Database inspection is explicitly requested. This is a useful app-owned boundary for an MCP adapter: applications can customize their internals while preserving a small inspection interface.

There are three limits to address:

- [The build script](../../starter/build.rs) embeds versions of selected dependencies from the lockfile, without exact Git source identities or a fingerprint of the current application source. An old binary can therefore describe an old build even while the AI edits newer files.
- Inspection shares application configuration validation. Missing or invalid configuration must not prevent the companion from searching files and identifying dependencies; return partial runtime information with a reason.
- Route inventory covers registered explicit routes. It does not prove network reachability or service health. Likewise, resolved Cargo features do not prove what a previously built binary contains.

Add optional build provenance to inspection: dependency identities, target, feature selection, and a source/build fingerprint. Preserve the existing JSON schema's additive compatibility. Keep framework availability, selected features, compiled support, configuration, and tested behavior as separate fields with `unknown`/`not_checked` states where needed.

### Suggested module boundaries

Keep the companion in developer tooling, outside the HTTP runtime dependency graph. Initially add modules behind `crates/bracel-cli/src/ai/`; split a reusable crate only when needed.

| Module | Responsibility |
| --- | --- |
| `project` | Find the application, selected Cargo target/features, resolved dependency identities, and local source roots. |
| `knowledge` | Load a compatible knowledge bundle; search Markdown, Rust documentation comments, examples and capability entries; return source references. |
| `generate` | Compose concise framework guidance and task skills, applying reviewed app overrides. |
| `agents` | Write each agent's instruction, skill and MCP configuration formats without replacing unrelated content. |
| `inspect` | Adapt an explicitly configured application inspection executable and validate its JSON contract and build provenance. |
| `mcp` | Expose bounded tools over stdio using the same services as the CLI. |
| `freshness` | Compare current inputs with the saved generation/index manifest and explain mismatches. |

Use the official Rust MCP SDK, `rmcp`, for protocol handling and local stdio transport. Pin a compatible SDK release during implementation. Keep protocol output on stdout and diagnostic logs on stderr. This avoids implementing MCP transport and schema handling from scratch. [Official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk), [MCP tools contract](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

### Resolve the framework before choosing guidance

Read structured Cargo metadata using `cargo metadata --format-version 1 --locked --offline`, supplying the project's selected feature and platform options. Traverse dependency identities from the selected application; do not merely scan crate names or assume the globally installed CLI's version equals the app's framework version. Preserve multiple versions and sources when they exist. Missing cached dependencies should produce a clear unresolved state. A deliberate fetch can make them available; an offline inspection should not silently change the lockfile. Metadata describes dependency resolution, while inspection of the built app describes compiled behavior. [Cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html).

Select knowledge according to the dependency source:

| Dependency source | Knowledge to use |
| --- | --- |
| Workspace/path dependency | The resolved local source directory and its current documentation, with content hashes for changed and untracked relevant files. |
| Git dependency | The resolved commit's source and knowledge bundle, using the locked commit identity. |
| Registry dependency, if Bracel later publishes one | Assets shipped with that exact crate version or a matching separately distributed bundle. |
| Missing exact knowledge | Report the gap; expose available source and clearly labeled fallback material. |

Keep one canonical `ai/` source directory in the framework repository and generate a portable knowledge bundle during release preparation. Include the capability catalog and the documentation/examples it references, preserving relative links. Consumers must be able to find this bundle in a Git checkout or release artifact without requiring the framework maintainer's filesystem. Test the bundle in the existing standalone consumer/export flow. Future crates.io support needs explicit packaging of these assets: repository-root documentation is not automatically part of a nested crate's package. [Current export script](../../scripts/export-starter.py), [Cargo packaging](https://doc.rust-lang.org/cargo/commands/cargo-package.html).

The globally installed companion is the reader/generator; knowledge is selected independently for each application. Older framework revisions that predate the bundle can use an explicitly labeled compatibility adapter or source-only search. Never quietly substitute the CLI's bundled newest APIs.

### Proposed commands and tools

These commands do not exist in the current CLI:

```text
bracel ai install               Configure selected agents and initial knowledge
bracel ai sync                  Refresh generated files and the local index
bracel ai sync --check          Report drift without writing; suitable for CI
bracel ai doctor --json         Report source, knowledge and build freshness
bracel ai search "job retries"  Search the selected version's knowledge
bracel ai mcp                   Serve the same information to agents over stdio
```

Start with five MCP tools:

| Tool | Useful output |
| --- | --- |
| `project_info` | Application identity, exact framework dependencies, selected features, source roots and freshness state. |
| `search_docs` | Bounded excerpts with path/URL, heading or symbol, line references, revision and content hash. |
| `capabilities` | Feature availability and links to its API, example, limitations and verification evidence. |
| `inspect_application` | Existing application route/schema/command inventory, plus build provenance and any configuration errors. |
| `doctor` | Stable diagnostic codes and specific steps needed to refresh or resolve missing information. |

The MCP process should invoke only the configured inspection command with fixed arguments and a timeout. Do not wrap arbitrary app commands behind an inspection tool: Bracel also has migration, worker and retry commands with side effects. Local documentation search should remain usable without launching or compiling the app. This boundary follows the existing [command parser](../../starter/src/cli/tooling.rs) and [application command registry](../../starter/src/cli/commands.rs).

### Store enough information to detect stale context

Commit agent selection, app-owned rules, canonical overrides and generated guidance for review. Keep caches, absolute machine paths and timestamps outside committed generated content so regeneration stays deterministic. A proposed `.bracel/ai.lock.json` should record:

- Companion version and knowledge schema version.
- Each resolved framework package's source, version and Git revision when applicable.
- Hashes of relevant manifests, the lockfile, canonical guidance, docs and app overrides.
- Selected target and feature settings.
- Generated file hashes and the files owned by the installer.

Track every generated file. Remove obsolete skills only if they are installer-owned and unchanged since generation. If someone edits a generated block or skill, report a conflict and provide a diff; preserve their edits. Use content hashes to detect branch changes and local edits even when package versions stay the same. A file watcher can reduce latency, but requests should validate relevant hashes so a missed watch event cannot silently return stale content.

## Keeping new features visible to the AI

### Framework contributor workflow

For each feature PR, require a capability entry, public documentation, a compiling example or executable acceptance scenario, and task guidance when the workflow changes. The entry should link to evidence rather than copying implementation details into several files. A changed-file rule can flag probable missing docs; a reviewer still checks their accuracy. Generated examples should be exercised against the same framework revision as the knowledge bundle.

Add an AI knowledge check to the existing [check script](../../scripts/check.sh) and [CI workflow](../../.github/workflows/ci.yml). Validate catalog references, regenerate owned outputs, check for drift, and run the independent consumer scenarios below. Publish the tested bundle with the corresponding framework release, retaining older bundles. Publishing a new bundle alone must not change the instructions of an app pinned to an older API.

### Application developer workflow

| Event | Refresh action |
| --- | --- |
| A feature changes in a local framework checkout | Re-read changed source/docs; invalidate affected search entries; sync if canonical guidance changed. |
| An application's framework dependency changes | Resolve dependencies, sync matching knowledge, then check for stale compiled inspection. |
| A branch switch changes manifests or rules | Detect hash differences on the next companion request; refresh or report the mismatch. |
| An app adds its own architectural convention | Commit an app-owned rule and refresh its rule index. |
| Agent instructions or available skills change | Reload/restart the agent session as supported by that client. |
| A documentation correction is published for an existing release | Optionally retrieve a separately versioned, API-compatible bundle for that exact release and record its identity. |

Document `bracel ai sync` as part of the project's dependency-update workflow and enforce `--check` in CI. Do not put editor-file generation or downloads in a Cargo build script. Cargo build scripts execute during builds and direct generated output into `OUT_DIR`; they are a poor place to modify an application's agent configuration. The existing build script may continue to emit build metadata. [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html).

### Availability is enforceable; model attention is not

Instruction files should direct the agent to call `project_info` at task start, search matching documentation before unfamiliar API work, and inspect current code/tests before changing behavior. The server should return freshness/provenance with each result and refuse to label mismatched documentation as current.

An instruction cannot guarantee that every agent will call a tool or discard facts already in its conversation. MCP makes tools available, while the agent decides when to use them. Session reloads, visible stale-state diagnostics, and executable verification are therefore part of the design. [MCP tool invocation model](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

### Acceptance scenarios for the first implementation

1. Two fixture consumers resolve different framework revisions. Each receives matching docs and capability availability; the older one is never told an unsupported new API is available.
2. Editing a public API and its docs in a path dependency changes search results without requiring a release. Deleting or renaming a document removes the old indexed result.
3. Switching branches or changing the lockfile produces a stale-state diagnostic until sync completes; repeated sync is deterministic.
4. Installer updates preserve handwritten instructions, unrelated MCP servers and app-owned rules; edited generated files produce conflicts instead of data loss.
5. A stale inspection binary is identified separately from fresh source knowledge; invalid app configuration still permits local docs search.
6. Missing cached dependencies, unavailable documentation and unsupported bundle schemas return actionable partial results without claiming freshness.
7. An exported standalone consumer finds its knowledge outside this workspace and works offline after initial dependency/bundle retrieval.
8. Representative agent tasks (an HTTP resource and a durable job) use valid APIs and pass focused E2E checks. Record the resolved revision, retrieved sources, generated diff and test outcome as the repeatable acceptance artifact.

Implement the command/resolution/generation path first, then local search and the five MCP tools. Add hosted search only when local search quality, distribution costs or cross-project use justify it. The initial success criterion is that the agent can identify the right code, retrieve its guidance, and detect stale context reliably.

## Research verification

This report combines official Laravel documentation, pinned Boost source, official Cargo/MCP references, and direct inspection of Bracel's current files. No framework implementation, dependency upgrade, publication or release was performed. Runtime checks (`scripts/check.sh` and `scripts/container-smoke.sh`) were not run for this research-only change; the acceptance scenarios above are proposed work, not passing test results.
