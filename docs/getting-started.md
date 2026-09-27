# Install, create and update a Bracel application

Bracel's CLI manages project creation, local development and framework upgrades.
New projects default to Docker development: install Git and Docker Desktop; Rust,
a C compiler and PostgreSQL run in containers. An editor is your choice.

## Install the CLI

The installers download a prebuilt executable and verify its release SHA-256 and
reported version before replacing an existing installation. No Rust installation
or administrator privileges are required to install the CLI.

**Release availability:** these installers require binary assets produced by the
`Package CLI` workflow. Existing v0.4.0 assets are not implied by this source
change. Until a release containing this workflow has been published, build this
checkout with `cargo install --path crates/bracel-cli --locked`.

For a release with binary assets, download `install.ps1` or `install.sh` from its
[GitHub release page](https://github.com/4H1R/bracel/releases). On Windows, run the downloaded script in PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

The default directory is `%LOCALAPPDATA%\Bracel\bin`. The installer adds it to
your user PATH. Open a new terminal afterward. `-Version 0.4.0` selects an exact
release; `-InstallDir DIRECTORY -NoPath` chooses a directory without changing PATH.

On Linux x64 or macOS (Intel or Apple silicon):

```bash
bash install.sh
export PATH="$HOME/.local/bin:$PATH"
```

Add that PATH line to your shell configuration for future terminals. Options:
`--version VERSION` and `--install-dir DIRECTORY`. Linux binaries are built on
Ubuntu 22.04; older glibc distributions and other architectures can build the CLI
with Cargo. The prebuilt CLI's platform does not imply application platform coverage.

## First project

```text
bracel setup
bracel new my-api
cd my-api
bracel dev
```

`setup` checks Git, Docker Compose and the running Docker engine. Missing tools
produce installation links and a nonzero exit code. It does not install system
software silently. Start Docker Desktop, then retry if the engine is unavailable.

`new` downloads the starter tag matching the CLI version. It renames the Cargo
package, executable references, source imports and lockfile entry; creates `.env`
from the example; and writes `.bracel/project.json`. It preserves `STARTER_VERSION`
and the `starter` Git remote. Existing destinations are never overwritten; failed
downloads or customization leave no partially created application directory.

The default ports are API 3000, PostgreSQL 5432, SMTP 1025 and Mailpit 8025. Project
creation chooses available alternatives when these ports are occupied. The CLI
prints the selected addresses. Ports can later be changed in `.bracel/project.json`;
native development also needs corresponding `.env` changes. A port can become busy
between project creation and startup; change its setting and retry in that case.

`dev` starts PostgreSQL and Mailpit, builds the application, applies pending local
migrations, starts the account email worker, and serves the API in the foreground.
The first run downloads the Rust image and dependencies. Later builds reuse named
Cargo and compilation caches. Restart `dev` after editing Rust source to rebuild.

Press Ctrl+C to stop the API. Run `bracel down` to stop the project's services.
Database and build volumes remain available for the next run. Removing a project
directory does not delete its Docker volumes.

Docker development uses its own local database and mail capture, overriding
`DATABASE_URL`, bind address and SMTP connection settings from `.env`. All published
ports bind to loopback. Mailpit and the application share a network namespace so
the framework's loopback-only local SMTP transport keeps working. Services and
volumes are isolated by project name plus a hash of the absolute project path.
Moving a project changes that identity; retain the old directory until any local
data has been exported or deliberately discarded.

## Native development

Install Rust through rustup and a C/C++ toolchain, then choose native mode:

```text
bracel setup --native
bracel new my-api --native
cd my-api
bracel dev
```

Inside an existing project, `bracel setup --native` saves the mode after successful
checks. `--native` or `--docker` on a development command overrides the mode for
that invocation. Windows uses the starter's native PowerShell launcher when present.

Native mode runs the application on the host and PostgreSQL/Mailpit in Docker.
Run `bracel run --native auth:mail-work` in a second terminal for account email.
With your own local database, use `bracel dev --native --no-services`.
Automatic native migrations require a loopback database URL without query parameters. `--no-migrate`
skips migrations; explicit `bracel run --native migrate` uses your configured URL.

The CLI reads `.env` as literal assignments, including quoted multiline values.
It does not execute shell expressions or expand variables. Process environment
values take priority. The older `scripts/dev.sh` still sources trusted shell files.

## Commands while developing

```text
bracel make resource Project --field name:string --crud
bracel run doctor --database --json
bracel run inspect --json
bracel cargo check --locked
bracel cargo test --locked
```

`run` forwards application commands and their arguments. `cargo` uses the selected
development environment. In Docker mode Cargo runs in a separate tools container;
it does not start database services or automatically configure a test database.
Database tests still need the disposable test environment required by the project.
Set `TEST_DATABASE_URL` in the process environment or `.env`; Docker commands
forward it unchanged, so its hostname must be reachable from the tools container.
The repository's complete check and container scripts remain the release gates.

## Update the CLI

```text
bracel self update
bracel self update --to 0.4.0
```

Without `--to`, the CLI selects the latest published GitHub release. An update verifies
checksums and the binary version before replacement and keeps `bracel.previous`
alongside the installed executable. It requires curl and published binary
assets for the selected release. Applications keep their selected dependencies.

## Update an application's framework

Commit the application's current work, then:

```text
bracel upgrade --check
bracel upgrade --to VERSION
```

The preview works with a dirty tree and changes no project files. It reports the
exact target commit, each dependency change, the release notes and any required
toolchain change. Applying an upgrade requires a clean Git working tree.

The CLI updates every supported direct Bracel dependency, including aliases,
development/build dependencies and target-specific declarations. Features and
optional flags are preserved. It resolves the lockfile and compiles all application
targets. A newer release toolchain updates the pinned channel while preserving
other toolchain settings. Failed resolution or compilation restores the original
manifest, lockfile and toolchain bytes. It never performs a Git reset.
When the compiler changes, also align your deployment Dockerfile's Rust builder
and any CI compiler pins; the CLI reports this step and preserves those application files.

Successful compilation is not proof that your business behavior is unchanged.
Review the diff and release notes, run your application's checks, adopt required
configuration/API changes, and commit the result. Database migrations are never
run during an upgrade. Apply reviewed forward migrations through your deployment
process. Application source, configuration, starter history and existing migrations
are preserved; starter changes are adopted selectively.

Automatic upgrades support standalone applications using the public framework Git
repository. Workspaces, path dependencies, custom forks and dependency patches
need manual updates. Numeric pinned Rust channels are required. Updating an older
application does not convert it to the latest starter.

If AI context is installed, the CLI refreshes it after a successful upgrade.
Context conflicts retain the successful dependency upgrade and explain how to fix
the generated guidance. For projects configured in Docker mode, `bracel ai`
automatically runs in the tools container, using its fetched sources. On first use
it compiles and caches the matching released CLI there. Use
`bracel ai install --agents codex,claude,cursor`, then reload the agent.
See [AI companion](ai.md).

## Verification

`scripts/lifecycle-e2e.py` runs the actual CLI against isolated Git release fixtures
and real Cargo projects, with recorded Docker orchestration. It covers project
naming, missing tools, previews, dirty trees, dependency aliases, failed upgrades,
environment parsing and failed/remote automatic migrations. `scripts/installer-e2e.py`
verifies installation and corrupt-download preservation using a local transport.
Both save repeatable JSON evidence and run through `scripts/check.sh`.

`scripts/lifecycle-container.py --bin PATH` exercises a real matching starter in
Docker: build, migrations, readiness, registration, profile, queued mail, stop and
restart. It removes its own disposable resources and retains JSON evidence and logs.
