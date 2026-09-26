# Distribution

The framework, CLI, integrations and starter use version 0.4.0. They are packaged by the checks, but are not published to crates.io. Public registry names have not been reserved. Keep their package versions aligned: the CLI derives its starter tag from its own package version, and export refuses mismatches before creating a destination.

The reference starter lives under starter/. Export a tested framework revision with:

~~~bash
python3 scripts/export-starter.py /absolute/new/directory FULL_FRAMEWORK_COMMIT
~~~

The destination must not exist or be inside the reference starter. Export reads a local Git archive of the selected commit, validates that snapshot's manifests, installs its root lockfile, and pins framework dependencies to that same revision while preserving their features. Dirty and untracked working-tree files are excluded. The completed export is moved into place atomically; `EXPORT_MANIFEST.json` records both revisions and SHA-256 hashes of the exported files. These hashes describe the initial export, before the required lockfile source update or application customization.

Commit coordinated, verified framework and starter changes locally before exporting them. The commit need not be published to create the export. Run cargo check in the export to update the framework's lockfile source, then run its checks before committing and tagging the matching package version in bracel-starter. For local verification, a per-command Git URL rewrite may point Cargo at the source checkout containing the exact commit. Keep the framework commit reachable on GitHub before distribution. Publish the starter tag only after its revision-pinned build passes; never move an existing release tag.

Both repositories are public and MIT-licensed. Git/Cargo and standalone CI can fetch the framework without private repository credentials. The build-image script vendors locked dependencies outside Docker, then builds without network access. No credentials enter the Docker build. Starter CI runs on pushes and pull requests.

Do not rewrite released migration history or replace an adopter's application with the starter. Framework improvements arrive through dependency updates; starter changes are reviewed and adopted selectively.

All optional workflow crates share the workspace release version; GitHub releases do not publish or reserve registry names. `cargo package --workspace --exclude bracel-starter --all-features` verifies the unpublished dependency graph together. The smoke script extracts the produced archives and tests an independent consumer using local registry patches. Starter export pins every direct `bracel-*` dependency to the selected revision. No release action is part of local verification.

## AI knowledge

Root `ai/` is the canonical framework knowledge source. Git consumers discover it in the
resolved revision's checkout. Before release, use `bracel ai bundle --output NEW_DIRECTORY`
against the tested application/dependency selection and preserve its integrity manifest
with that revision. A consumer may use that directory via `knowledge_bundle` in its AI
configuration. Bundles are checked against resolved package identities, features and source
hashes; a newer CLI never substitutes newer framework APIs. Nested crate archives do not
include root documentation automatically. See [the AI companion](ai.md).
