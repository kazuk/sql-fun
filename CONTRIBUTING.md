# Contributing to `sql-fun`

Thank you for considering a contribution to
[`sql-fun`](https://github.com/kazuk/sql-fun).

Small, focused changes are welcome. Bug fixes, documentation improvements, test
coverage, examples, scripts, and experiments can all be useful when they make
working with SQL and `sql-fun` easier.

## Getting Started

1. Fork or clone the repository.
2. Make sure the PostgreSQL submodule is available if your change needs it.
3. Install `direnv` or an equivalent tool that can load [`.envrc`](.envrc)
   files.
4. Install Docker CLI access.
5. Enable the repository environment.
6. Make the Rust toolchain required by the workspace available.

This repository relies on [`.envrc`](.envrc) to set local development
environment values, including `GIT_ROOT`, `SQL_FUN_HOME`, local script paths,
and Git hooks. With `direnv`, allow the environment after cloning:

```sh
direnv allow
```

If you use another environment loader, make sure it applies the same
[`.envrc`](.envrc) settings before running build, test, or development scripts.

Docker is required for workflows that extract built-in function and extension
definitions from official PostgreSQL images.

A recent stable Rust toolchain is usually sufficient for local development. The
workspace `rust-version` in [Cargo.toml](Cargo.toml) defines the minimum
supported Rust version checked by CI.

```sh
rustup toolchain install stable
```

Install the Rust tools used by project scripts:

```sh
cargo install --locked --force \
  cargo-llvm-cov mdbook cargo-deny tera-cli taplo-cli cargo-nextest \
  sccache cargo-sort cargo-edit cargo-expand cargo-binstall typos-cli
```

`mdbook` is required by the documentation build. The `doc` check-flow step
builds both the Rust API documentation and the user manual.

`cargo-expand` is a runtime prerequisite of `sql-fun check`, which invokes
`cargo expand` to read the generated query stubs. Install it before using the
command and ensure it is available on `PATH`.

Contributors working on Debian packaging also need `cargo-deb`. The standard
CI/development image includes it. When developing outside that image, install
it and ensure Cargo's binary directory is on `PATH`:

```sh
cargo install --locked cargo-deb
cargo deb --version
```

For the Linux distribution build command, native packaging prerequisites, and
artifact validation, see [Linux packaging](docs/internal-rules/linux-packaging.md).

Some tools, especially `cargo-deny`, may require a recent stable Rust toolchain
to install. If installation fails, update stable first and try again.

Install `graphify` for the graph-index update performed by the standard check
flow. The package name has two trailing `y` characters, while the installed
command is `graphify`:

```sh
uv tool install graphifyy
```

Make sure the tool's executable directory is available on `PATH`. See the
[Graphify installation instructions](https://github.com/Graphify-Labs/graphify#install)
for platform-specific setup.

Codex contributors also need [`jq`](https://jqlang.org/download/) on `PATH`.
The repository's Codex pre-tool-use policy uses it to read hook input. The
standard CI/development image already includes `jq`.

The full `check-test-flow` includes a `skill-lint` step that validates repository
skills with `skill-validator`. Contributors running this flow outside the project
CI image need the tool available on `PATH`; see the upstream
[installation instructions](https://github.com/agent-ecosystem/skill-validator#install).
The CI image currently pins version 1.6.2 for reproducibility.
The validation accepts a skill-root README when `SKILL.md` links to it, so skills
can include maintainer-facing guidance without producing a structural warning.

The optional `cli-contract` check-flow step evaluates source-owned Scrut CLI
contracts against the locally built `sql-fun` executable. It requires the
`scrut` DotSlash wrapper and `dotslash` executable to be available on `PATH`.

Source builds of the SQL analyzer require a C compiler and assembler for the
`stacker`/`psm` stack-growth backend, in addition to the native prerequisites of
`pg_query`. On Ubuntu, install `build-essential` and `clang`; both are already
included in the CI/development image. Cross-compilation requires the corresponding
target toolchain. Stack growth adds no executable prerequisite at CLI runtime.

Some Rust dependencies may require additional native build dependencies. Package
names vary by operating system and distribution, so install the missing packages
reported by the build errors for your environment.

Useful local scripts live under [`.local/bin/`](.local/bin/). The repository
environment adds this directory to `PATH`.

## Release workflow validation

The standard `version-materialization` step includes offline release publication
contract tests. They require the existing Bash, Git, Cargo, and `jq` development
setup and do not publish packages or deploy Pages.

For additional local workflow syntax validation, optionally install
[`actionlint` 1.7.12](https://github.com/rhysd/actionlint/releases/tag/v1.7.12).
Download the archive for your platform from that release, verify it against the
published checksums, and put the extracted executable on `PATH`. It is an
optional contributor tool, not a runtime or standard CI-image prerequisite.
Validate the release workflow without requiring optional ShellCheck integration:

```sh
actionlint -shellcheck='' .github/workflows/release-publish.yml
```

## Development Workflow

Create a branch for your change:

```sh
git checkout -b fix/short-description
```

Keep changes focused and prefer small pull requests. If a change touches several
independent areas, split it when practical.

You do not need to rewrite history just to make the work look effortless. A
development history may show the real path taken, including investigation,
corrections, and follow-up fixes. Keep commits understandable, but do not hide
useful context by over-polishing the history.

Do not commit directly to `main`. The repository Git hooks block direct commits
to `main` when the local environment is enabled.

Before making broad design changes, open an issue or draft pull request to
discuss the direction.

## Code Style

Rust code should follow the project coding guidelines:

- [docs/internal-rules/rust-coding-guideline.md](docs/internal-rules/rust-coding-guideline.md)

In general:

- Prefer simple, maintainable solutions over unnecessary abstraction.
- Keep public APIs as small as practical.
- Place focused unit tests near the implementation they cover.
- Do not use wildcard imports.
- Do not import derive macros just to use them unqualified; prefer fully
  qualified derive paths such as `#[derive(serde::Deserialize)]`.
- Do not import names that hide their origin or make non-`std` items look like
  standard-library types. Avoid generic aliases such as `Result`, and rename
  ambiguous error types such as `Error` at the import site.

Run formatting before submitting:

```sh
.local/bin/check-test-flow --step=fmt
```

To check formatting without rewriting files:

```sh
.local/bin/check-test-flow --step=fmt --fmt-check
```

## Markdown linting

Install the same rumdl version used by the development/CI image:

```sh
cargo install --locked rumdl --version 0.2.76
```

Run the Markdown checks from the repository root:

```sh
.local/bin/check-test-flow --step=markdown-lint
```

This excludes Git submodules, whose Markdown is maintained externally, while
checking this repository's Markdown files. The standard check flow uses the same
exclusions.

Use `rumdl fmt <path>` to apply automatic fixes to your Markdown files, then
review the diff and rerun the check.

## Generated Files

Do not edit generated files directly.

The following file is generated:

- `sql-fun-sqlast/src/syn/generated.rs`

It is generated by [sql-fun-sqlast/generate](sql-fun-sqlast/generate). The
generation logic and templates live in the [xbuild](xbuild) project. Update the
generator or templates instead of editing the generated file by hand.

## Tests and Checks

The standard local verification flow is:

```sh
.local/bin/check-test-flow
```

The script runs the workspace checks, tests, formatting, linting, documentation
build, dependency checks, API spec generation, and typo checks. After all other
steps pass, it updates the repository graph index with `graphify update .`.

`check-test-flow` prioritizes consistent behavior between CI and local runs.
Both use the shared defaults in [`.cargo/config.toml`](.cargo/config.toml):
incremental compilation is disabled, and the `dev` and `test` profiles use
`debug = "line-tables-only"` to reduce disk usage while retaining file and line
information in backtraces.

When local debugging requires full debug information, edit `.cargo/config.toml`
and set `debug = "full"` in both `[profile.dev]` and `[profile.test]`. Restore
these shared defaults before running final checks or committing.

If you prefer incremental builds locally, set `CARGO_INCREMENTAL=1` in your
personal shell environment (for example, `export CARGO_INCREMENTAL=1` in your
shell startup file). This overrides the shared default without changing
`.cargo/config.toml`. Unset it when verifying with the same settings as CI.

For faster iteration, run only selected steps:

```sh
.local/bin/check-test-flow --step=affected
.local/bin/check-test-flow --step=workspace
.local/bin/check-test-flow --step=proc-macro-tests
.local/bin/check-test-flow --step=lint
.local/bin/check-test-flow --step=doc
.local/bin/check-test-flow --step=graphify
.local/bin/check-test-flow --step=packaging
```

`packaging` is an opt-in step and is not part of the default check flow. It
builds Linux distributions and always validates them before publishing outputs.
See [Linux packaging](docs/internal-rules/linux-packaging.md) for prerequisites.

List available steps with:

```sh
.local/bin/check-test-flow --list-steps
```

Some checks require extra tools, such as `cargo-nextest`, `cargo-deny`,
`cargo-sort`, `cargo-llvm-cov`, `cargo-edit`, `cargo-expand`, `taplo`, `typos`,
`jq`, and `graphify`.

## PostgreSQL extraction integration

Run this check in an environment with Python 3, `psql` (PostgreSQL client tools),
Docker CLI, and access to a Docker daemon. Build the CLI, then run the check with
a new or empty output directory:

```sh
cargo build -p sql-fun-cli --bin sql-fun
.local/bin/test-postgres-extraction --binary target/debug/sql-fun \
  --output /tmp/sql-fun-extraction-validation
```

The script also exercises PostgreSQL initialization from Docker and an existing
server, reruns, and source/version mismatch diagnostics. To validate initialization
followed by semantic loading of builtin and user schemas, run the Docker integration
test explicitly:

```sh
cargo test -p sql-fun-cli --test initialization -- --ignored
```

Set `SQL_FUN_TEST_DOCKER_HOST` and `SQL_FUN_TEST_DOCKER_BIND` when the Docker host
requires specific connectivity addresses. Set `SQL_FUN_TEST_BINARY` to an installed
CLI executable to test that binary instead of the workspace CLI. CI builds this
test harness in the builder image and runs it against the installed package on
the Docker-capable Ubuntu runner.

To check an installed CLI, supply its executable path with `--binary`.
Use `--host` and `--bind` when the Docker host is reached through a specific
interface instead of loopback.

## Inspecting Coverage

To inspect coverage, you can run:

```sh
.local/bin/update-stats
```

The report is written to `stats/coverage.txt`, which is ignored by Git.

Run `check-test-flow` and `update-stats` sequentially; their tests share
temporary build directories.

## Commit Messages

Use Conventional Commits for commit messages.
The repository Git hooks also enforce this format when the local environment is
enabled.

Examples:

```text
fix(sqlast): handle quoted identifiers in column lookup
feat(cli): add schema inspection output
docs: clarify contribution workflow
test(core): cover virtual foreign key resolution
```

Common types include:

- `fix`
- `feat`
- `docs`
- `test`
- `refactor`
- `chore`

## AI-assisted Contributions

### Optional agent harness

This repository may enable an agent-only harness from [`.envrc`](.envrc) when
it detects a supported agent environment marker. The harness is an optional
repository-provided guardrail for maintainer and operator observability; it is
not a requirement for contributors to use a particular AI tool or setup.

Contributors who do not want their local agent to receive the harness may
locally comment out or disable the relevant agent-detection and `PATH` setup in
`.envrc`. This is a personal environment change and must not be included in a
pull request.

Support for additional agent runtimes is welcome. A contribution may add a
positive environment-marker detection path so the same harness applies to that
runtime, provided it does not weaken or broaden the existing Codex detection.

AI-assisted contributions are allowed, but AI tools are not accepted as authors
or co-authors.

Do not add `Co-authored-by` trailers for AI tools, agents, or services. A
contribution submitted to this project is considered the responsibility of the
human submitter.

The submitter must review, understand, test, and be able to explain and revise
the submitted change so that the project can reasonably maintain it after
acceptance. AI assistance is not an excuse for incorrect behavior, unclear
provenance, licensing issues, broken tests, or low-quality code.

If a contributor cannot take responsibility for the change as their own work,
they should not submit it.

## Issue and Pull Request Labels

Repository labels have defined workflow semantics. Before creating or updating
an issue or pull request, read
[docs/internal-rules/issue-pull-request-labels.md](docs/internal-rules/issue-pull-request-labels.md).

In particular, distinguish preparation state (`not-ready`), concrete dependency
state (`blocked` / `blocker`), and work that currently requires a human-only
operation (`require-human-work`).

## Pull Requests

Before opening a pull request:

1. Rebase or merge the latest target branch as needed.
2. Run [`.local/bin/check-test-flow`](.local/bin/check-test-flow) without
   arguments and make sure it passes.
3. Make sure generated files are updated only through their generator.
4. Write a clear summary of the change and include test results.

Good pull requests explain:

- What changed.
- Why the change is needed.
- How it was tested.
- Any limitations, follow-up work, or compatibility concerns.

## CI Image Changes

Changes to `ci/github-actions/Dockerfile` and related CI image tooling use a
dedicated review and publication workflow.

Before changing the CI image, read
[ci/README.md](ci/README.md) and follow the documented `ci-image` branch and
image adoption procedure.

## Dependency Documentation

When checking Rust dependency documentation, use the resolved version from
[Cargo.lock](Cargo.lock) first. If the dependency is not locked, inspect the
relevant [Cargo.toml](Cargo.toml) version requirement before looking up
documentation.

## Getting Help

If you get stuck while setting up the project or preparing a contribution, open
a question in the
[Q&A category of GitHub Discussions](https://github.com/kazuk/sql-fun/discussions/categories/q-a).

## Reporting Issues

When reporting a bug, include:

- The `sql-fun` version or commit.
- The command or code that failed.
- The SQL input, schema metadata, or configuration involved.
- The expected behavior.
- The actual behavior, including error output when possible.

For feature ideas or requests, read
[docs/feature-planing/README.md](docs/feature-planing/README.md) first. Then
describe the use case and the workflow you want to improve.
