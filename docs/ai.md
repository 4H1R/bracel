# Bracel AI companion

The `bracel ai` commands give coding agents documentation and inspection results matched
to the application's resolved framework dependencies. They run locally and work offline
after Cargo dependencies have been fetched. They do not upgrade dependencies.

## Set up an application

From the application's root, with a current lockfile and dependencies already fetched:

```bash
bracel ai install --agents codex,claude,cursor
bracel ai doctor
bracel ai search "validated request"
```

For the framework workspace, select the reference application:

```bash
cargo run -p bracel-cli -- ai install --manifest-path starter/Cargo.toml
```

The installed `bracel` executable must be on the agent's PATH. Run the agent in the project
root. On Windows, the repository's `scripts/windows.ps1` launcher configures native tools;
WSL is also usable. Run the companion and configured application binary in the same
environment. Restart/reload the agent after installation
or changes to instructions and skills.

`install` accepts `--features a,b`, `--no-default-features`, and `--target TRIPLE`. These
choices are saved in `.bracel/ai.json`; subsequent commands use them. Cargo metadata
describes resolution, while an application's inspection describes its compiled features.
When using a workspace, Cargo can unify features across members; use compiled inspection
to determine which capabilities a particular binary contains.

## Commands

| Command | Behavior |
| --- | --- |
| `ai install` | Configure selected agents and generate their initial context. |
| `ai sync` | Re-read dependency resolution, source, docs and app overrides; refresh generated files. |
| `ai sync --check` | Read-only check; exit 1 if context is stale. |
| `ai sync --dry-run` | Preview paths that would change. |
| `ai info` | Report dependency identities, source roots and generation freshness. |
| `ai capabilities` | List catalog definitions and availability in dependency resolution. |
| `ai search "query" --limit 8` | Return bounded source/document excerpts, paths, line references and hashes. |
| `ai doctor` | Diagnose instruction freshness and, when configured, compiled application provenance. |
| `ai inspect` | Run the explicitly configured application inspector and check build provenance. |
| `ai bundle --output NEW_DIRECTORY` | Write a portable framework knowledge bundle and integrity manifest. |
| `ai mcp` | Serve the five inspection/search tools over stdio. |

Every command accepts `--root DIRECTORY`. CLI results are JSON (`--json` is also accepted).
Exit 0 indicates success, 1 a negative check result, and 2 an invalid request or failed
operation. MCP reserves stdout for protocol messages and writes startup failures to stderr.

## Generated files and ownership

| Agent | Instructions | Skills | MCP configuration |
| --- | --- | --- | --- |
| Codex | Managed block in `AGENTS.md` | `.agents/skills` | `mcp_servers.bracel` in `.codex/config.toml` |
| Claude Code | Managed block in `CLAUDE.md` | `.claude/skills` | `mcpServers.bracel` in `.mcp.json` |
| Cursor | `.cursor/rules/bracel.mdc` | `.cursor/skills` | `mcpServers.bracel` in `.cursor/mcp.json` |

The installer preserves surrounding instruction text and unrelated MCP servers. It checks
every owned file/section before writing and reports a conflict if generated content was
edited. Previously generated skills are removed only when still unchanged. Changes can
be reviewed with Git. Put intentional customizations in the canonical override directories
and restore only the conflicting generated section before syncing.

Commit `.bracel/ai.json`, `.bracel/ai.lock.json`, generated guidance/skills and app rules.
The lock records source and knowledge hashes, dependency versions/sources, selection
settings, generator version and owned sections. Machine-specific source paths appear in
inspection output, not the committed generation manifest. Keep inspection executable paths
relative when possible. Generated configuration files may need explicit `git add` if your
global ignore rules exclude agent directories.

## Canonical knowledge and application rules

The framework stores `ai/catalog.json`, `ai/guidelines/*.md` and
`ai/skills/<name>/SKILL.md`. Catalog entries identify an owning crate, API, implementation
status, source, docs, example, verification reference, optional required feature and limits.
References are validated; a reference to a test is not a claim that it passed on your machine.

Applications add Markdown under `.ai/guidelines` and `.ai/skills`. Matching relative paths
override framework content. This first implementation supports Markdown skill entrypoints
at `<name>/SKILL.md`; scripts and auxiliary skill assets are not installed. Keep each
entrypoint self-contained and use version-aware search for detailed references.

Application rules live in `.ai/rules/*.md` with a YAML paths list:

```markdown
---
paths:
  - src/features/billing/**
---
Store money as integer minor units and keep currency explicit.
```

Sync generates `.ai/rules/index.md`; agents are instructed to read all matching rules.
The model performs path matching. The companion does not execute rule content or change
application code. Existing rules remain application-owned.

## Freshness and search

Each request re-runs locked, offline Cargo resolution and reads current source/document
content. There is no persistent search cache to become stale: the bounded lexical index
is built per request. The current implementation matches query terms in source/document
chunks and ranks matching chunks. It returns at most 20 excerpts. Design/research files
are labeled as design references.

Git dependencies use their resolved source checkout and locked revision; path dependencies
include current uncommitted files. A missing catalog produces an explicit source-only
warning. Unsupported schemas and unresolved offline dependencies produce errors. No newer
framework bundle is silently substituted. Optional capabilities retain separate resolved,
compiled, configured and verification states.

After dependency changes, branch switches, source/docs edits or rule updates, run:

```bash
bracel ai sync
bracel ai sync --check
```

Search already sees local edits before sync; sync refreshes the agent files and their
freshness manifest. Client sessions may still retain old instructions until reloaded.
The protocol exposes tools and instructions, but cannot force a model to use them.

## Inspect a compiled application

Set `inspection_executable` in `.bracel/ai.json` to the trusted application executable.
The companion passes only `inspect --json`, with a timeout (10 seconds by default,
configurable from 1 to 60) and a 2 MB output limit. It does not source `.env`; provide the
application environment through the process that starts the companion. Documentation
search remains available without valid application configuration.

The starter includes `application.build_provenance`: hashes of app source, the lockfile,
local framework dependencies, compiled feature names and target. Inspection compares
these to the checkout. Missing provenance is unknown, and mismatched source is stale.
A source match does not establish service health; inspect compiled/configured capability
fields separately. Provider verification and migration checks remain explicit app work.

## Bundles and distribution

A bundle contains framework source/docs, canonical AI content and the referenced examples
and checks. `bundle.json` records dependency identities and content hashes. Copy it into a
consumer project, set `knowledge_bundle` to its relative directory in `.bracel/ai.json`,
then sync. Loading rejects mismatched package identities/features or changed bundle files.
This supports distributing a bundle alongside a release without a hosted search service.
Bundle creation currently requires a single shared framework source root.

The current Git distribution already makes root `ai/` available in the resolved checkout.
Release preparation should run the bundle command against the tested dependency selection
and retain its artifact with that revision. It does not publish anything. Future registry
distribution must supply an exact compatible bundle; nested crate archives do not include
root repository documentation automatically.

## Contributor and CI workflow

When a feature changes, update code, public docs, its capability entry and applicable task
guidance together. Keep examples executable and add failure scenarios before implementation.
`scripts/check-ai.py` validates canonical references and skill frontmatter. Its optional
`--base REVISION` gate flags feature diffs with no documentation/knowledge changes; a reviewer
still checks semantic accuracy. PR CI invokes it against the base commit.

`scripts/ai-smoke.sh` builds the CLI, runs independent consumers and MCP acceptance checks,
writes `target/ai-e2e.json`, and checks that generated context is current. It is part of
`scripts/check.sh`. CI saves the JSON artifact even if a later check fails. Run the normal
framework and container checks as well. See [acceptance scenarios](ai-acceptance.md).
