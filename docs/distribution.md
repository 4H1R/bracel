# Distribution

The framework, CLI, integrations and starter use version 0.3.0. They are packaged by the checks, but are not published to crates.io. Public registry names have not been reserved. Keep their package versions aligned: the CLI derives its starter tag from its own package version, and export refuses mismatches before creating a destination.

The reference starter lives under starter/. Export a tested framework revision with:

~~~bash
python3 scripts/export-starter.py /absolute/new/directory FULL_FRAMEWORK_COMMIT
~~~

The destination must not exist or be inside the reference starter. Export validates manifests first, copies application code/docs/tests/assets, installs the root lockfile, and pins normal, development and optional framework dependencies to an exact Git revision while preserving their features. Run cargo check in the export to update the framework's lockfile source, then run its checks before committing and tagging the matching package version in bracel-starter. Keep the framework commit reachable on GitHub. Publish the starter tag only after its revision-pinned build passes; never move an existing release tag.

Both repositories are public and MIT-licensed. Git/Cargo and standalone CI can fetch the framework without private repository credentials. The build-image script vendors locked dependencies outside Docker, then builds without network access. No credentials enter the Docker build. Starter CI runs on pushes and pull requests.

Do not rewrite released migration history or replace an adopter's application with the starter. Framework improvements arrive through dependency updates; starter changes are reviewed and adopted selectively.

All optional workflow crates share the workspace release version; GitHub releases do not publish or reserve registry names. `cargo package --workspace --exclude bracel-starter --all-features` verifies the unpublished dependency graph together. The smoke script extracts the produced archives and tests an independent consumer using local registry patches. Starter export pins every direct `bracel-*` dependency to the selected revision. No release action is part of local verification.
