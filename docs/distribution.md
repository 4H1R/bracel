# Distribution

The framework, CLI and starter use version 0.1.1. They are packaged by the checks, but are not published to crates.io. Public registry names have not been reserved. Keep their package versions aligned: the CLI derives its starter tag from its own package version, and export refuses mismatches before creating a destination.

The reference starter lives under starter/. Export a tested framework revision with:

~~~bash
python3 scripts/export-starter.py /absolute/new/directory FULL_FRAMEWORK_COMMIT
~~~

The destination must not exist or be inside the reference starter. Export validates manifests first, copies application code/docs/tests/assets, installs the root lockfile, and changes the framework dependency from a local path to an exact Git revision. Run cargo check in the export to update the framework's lockfile source, then run its checks before committing and tagging the matching package version in bracel-starter. Keep the framework commit reachable on GitHub. Publish the starter tag only after its revision-pinned build passes; never move an existing release tag.

The initial repositories are private. Local Git/Cargo requires access to both. Standalone CI needs separately configured read access to the framework; no credentials or CI secrets are provisioned by repository creation. The build-image script vendors locked dependencies outside Docker using the caller's ordinary Git access, then builds without network access. No credentials enter the Docker build. Once Bracel is public or published on crates.io, ordinary unauthenticated CI can fetch it.

Do not rewrite released migration history or replace an adopter's application with the starter. Framework improvements arrive through dependency updates; starter changes are reviewed and adopted selectively.
