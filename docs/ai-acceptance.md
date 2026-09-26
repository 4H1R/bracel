# AI companion acceptance contract

Write and run these scenarios before treating the companion as complete. The executable
acceptance runner is `scripts/ai-e2e.py`; it emits a JSON evidence artifact containing
commands, results and fixture revisions. Fixtures live outside the framework workspace.

## Failure cases

- An old Git dependency receives a newer API because the installed CLI is newer.
- A renamed dependency, disabled optional crate or different target is misidentified.
- Missing offline dependencies or unsupported catalog schemas are reported as current.
- A changed, removed or untracked local document returns stale search results.
- Branch, lockfile, feature or local rule changes leave generated instructions stale.
- A second sync changes generated files without changed inputs.
- Installation replaces handwritten instructions, unrelated MCP servers or edited skills.
- Removing a skill deletes a file that was changed by the application owner.
- Symlinks or unsafe catalog paths escape the project during reading or generation.
- A running MCP process retains old context after a source edit.
- Invalid MCP arguments, unknown tools, command failures, timeouts or oversized output
  break protocol stdout or expose arbitrary process execution.
- Missing application configuration prevents documentation search.
- An old compiled application is described as matching the current working tree.
- A knowledge bundle depends on paths in the maintainer's workspace.
- Feature code changes without matching knowledge updates pass the CI drift gate.

## Required evidence

Exercise real CLI processes and an MCP initialize/list/call conversation. Create two
committed minimal framework versions and independent consumers pinned to each commit.
Exercise a mutable path dependency, all supported agent adapters, local overrides and
rules, stale/deleted/edited outputs, source-only fallback, offline failures, runtime
inspection and portable bundle verification. Preserve the fixture commit identities and
results in the artifact. Run framework checks and container smoke separately; these
scenarios do not replace HTTP, job, database or packaged-consumer verification.
