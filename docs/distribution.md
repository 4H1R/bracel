# Distribution

The framework and CLI use version 0.1.0. They are packaged by the checks, but are not published to crates.io. Public registry names have not been reserved. Releases must keep library, starter and CLI template versions aligned.

The reference starter lives under starter/. Export a tested framework revision with:

~~~bash
python3 scripts/export-starter.py /absolute/new/directory FULL_FRAMEWORK_COMMIT
~~~

The destination must not exist. The export copies application code/docs/tests, installs the root lockfile, and changes its framework dependency from a local path to an exact Git revision. Run cargo check in the export to update the framework's lockfile source, then run its checks before committing and tagging v0.1.0 in bracel-starter. Keep the framework commit reachable on GitHub. Publish the starter tag only after its revision-pinned build passes.

The initial repositories are private. Local Git/Cargo requires access to both. Standalone CI needs separately configured read access to the framework; no credentials or CI secrets are provisioned by repository creation. The build-image script vendors locked dependencies outside Docker using the caller's ordinary Git access, then builds without network access. No credentials enter the Docker build. Once Bracel is public or published on crates.io, ordinary unauthenticated CI can fetch it.

Do not rewrite released migration history or replace an adopter's application with the starter. Framework improvements arrive through dependency updates; starter changes are reviewed and adopted selectively.
