## Framework boundaries

Applications own domain features, state, configuration, migrations and process lifecycle.
Bracel supplies reusable HTTP, auth, query and optional workflow mechanics. Resolve
the application's selected dependencies before choosing an API; a globally installed
CLI does not identify the application's framework revision.

Use the capability catalog to distinguish implemented APIs, optional features and
recipes. Read the returned source/example references. Resolved dependencies, compiled
features, configured services and verified behavior are separate facts.

For feature changes, update the canonical documentation, capability entry and executable
example in the same change. Run the repository's authoritative checks and report actual
results. Keep application conventions in `.ai/rules` with a `paths` YAML list. Use
`.ai/guidelines` or `.ai/skills` for intentional overrides of generated guidance.
