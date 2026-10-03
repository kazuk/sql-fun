# crates.io publishing trial

The disposable `.github/workflows/crates-io-publish-trial.yml` workflow proves
the crates.io publication path that will later be adopted by the release
workflow in issue #259. Its reader is the release maintainer.

The trial has a fixed Trusted Publisher identity:

- GitHub owner: `kazuk`
- repository: `sql-fun`
- workflow filename: `crates-io-publish-trial.yml`
- GitHub environment: none

Before its first run, an owner of every existing publishable sql-fun crate must
create a crates.io Trusted Publisher entry with those values. This is a
crates.io account operation; it cannot be performed by repository automation.
The workflow receives a temporary token through
`rust-lang/crates-io-auth-action@v1` and `id-token: write`.

Trusted Publishing cannot create a crate that does not yet exist on crates.io.
For that one-time bootstrap case only, configure `CRATES_IO_BOOTSTRAP_TOKEN`
as a GitHub Secret.
The workflow uses the bootstrap token only after confirming that the crate is
absent; a crate that exists but lacks the target version always uses OIDC. If
the secret is missing when an unpublished crate is reached, it fails and
names the crate that needs bootstrap. After a bootstrap publish, configure its
Trusted Publisher entry, enable crates.io's Trusted Publishing requirement, and
remove the secret. It must be absent during normal operation.

## Running the trial

The workflow is manual-only. After it is present on the default branch, select
**Run workflow**, provide a new SemVer-safe snapshot name, and leave
`stop_after` empty for a complete run.

To exercise resume behavior, run with `stop_after` set to a package name from
the generated manifest. The workflow intentionally fails after that package is
published. Run it again with the same snapshot name and no stop point. Existing
crate/version pairs are skipped and unpublished pairs continue in workspace
dependency order.

The publication job materializes versions with
`.local/bin/materialize-release-versions`, then publishes each crate from that
source revision in dependency order. Cargo creates the `.crate` source package
and may verify it by compiling unpacked source. The workflow uses `-vv` to
retain that observed behavior in its log; it does not parse the log or block
compiler probes.

The workflow summary records the snapshot used and the no-compilation guard.
Record the run URL, resulting crate versions, resume result, and any Cargo
behavior that requires a different trial-only compiler wrapper in issue #300.

## Follow-up

When #259 adopts the proven publication steps, configure a new Trusted
Publisher entry for its final workflow filename before deleting this trial
workflow. Remove `.local/bin/test-packaging` as part of that adoption; it must
not remain as a competing version-materialization path.
