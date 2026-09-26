# AI companion verification

Checked on 2026-09-27 against the local 0.3.0 workspace. This record covers the companion;
the combined framework, generator, database and container checks are recorded separately
in [repository verification](verification.md).

## Passing focused checks

- `cargo clippy -p bracel-cli -- -D warnings` and CLI compilation passed on Rust 1.98.1.
- `python3 scripts/ai-e2e.py --bin /tmp/bracel-ai-target/debug/bracel` passed 11 scenario
  groups with 70 recorded CLI/protocol operations. Fixtures are outside the workspace.
- Real starter inspection passed with app source, lockfile and local dependency hashes
  matching its compiled provenance. The configured database address was unreachable;
  offline inspection did not connect to it.
- All five canonical skills passed the skill frontmatter validator.
- `scripts/check-ai.py`, its `--base HEAD` check, and `git diff --check` passed.
- `bracel ai sync --check` passed after regeneration.
- The same 11 fixture groups passed with the native Windows executable. Native offline
  workspace resolution required `cargo fetch --locked` for all platforms; a prior
  target-specific fetch was insufficient. Evidence is in
  `target/native-windows/ai-e2e.json`.

## Behavior exercised

The acceptance runner builds two committed framework revisions and independent consumers
using a renamed dependency. It verifies exact revision selection, optional and planned
capability status, selected features, dependency changes, live untracked docs, deletion,
idempotent sync, all three agent adapters, handwritten content preservation, generated
conflicts, retired skills, application overrides and rule indexing.

It also verifies current/stale/unknown binary provenance, fixed inspector arguments,
invalid JSON/contracts, timeout and output limits, portable bundle consumption, changed
or unlisted bundle content, unavailable offline dependencies, source-only fallback,
unsupported catalog schemas and unsafe paths. Symlink checks run where the platform
supports creating the fixture links.

An actual MCP stdio session performs initialization, tool discovery, all five tool calls,
live search after a file change, invalid input and an unknown tool. Protocol reads have a
deadline. The artifact includes the fixture revisions, commands, expected exit outcomes
and MCP responses. Reproduce it with `scripts/ai-smoke.sh`; CI uploads `target/ai-e2e.json`.
The development-run artifacts are `.scratch/ai-e2e.json` and
`.scratch/ai-real-inspection.json`.

## Limits

The companion uses a bounded local lexical index rebuilt on each request. It does not
host an embeddings service. Skills currently support self-contained Markdown entrypoints.
Knowledge bundles require one shared framework source root and exact package selection.
Cargo resolution is explicitly separate from compiled feature evidence. Tool availability
does not prove a model used a tool; application acceptance checks remain necessary.
