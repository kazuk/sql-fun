# Release distribution

This guide is for maintainers publishing a prepared sql-fun product release.
Issue #259 prepares the immutable tag, materialized source, draft GitHub Release,
and its assets. A maintainer publishes the draft as the final approval gate.
`.github/workflows/release-publish.yml` then handles distribution (#260):

1. Validate the product release tag and resolve its immutable source commit.
2. Publish all publishable workspace crates to crates.io.
3. Deploy the existing manual to Pages for the latest stable release only.

Announcements, when configured separately, must follow successful distribution.
The GitHub Release can be visible before crates.io or Pages completes.

## Release identity and source

`release.tag_name` is authoritative. The shared
`.local/bin/parse-release-version` parser accepts `vX.Y.Z`,
`vX.Y.Z-snapshot`, and `vX.Y.Z-snapshotN` (where N is a sequence of digits).
Numeric version components must not have leading zeros. Other prerelease names,
build metadata, and invalid tags fail before distribution. The upstream release
branch suffix must equal this product tag; future branch consumers can use the
same parser on that suffix.

Classification does not use `release.target_commitish`, `release.prerelease`,
or a workflow input. Snapshot releases never enter the Pages job.

The tag identifies the product release, independently of individual crate
versions. Every publication job checks out the commit resolved from that tag.
`.local/bin/release-crate-manifest` reads the complete publishable workspace
package/version set from Cargo metadata there. It never derives crate versions
from the product tag, rematerializes versions, or publishes a mutable branch tip.

## crates.io setup and permissions

Before using the production workflow, an owner of each existing publishable
crate must configure this crates.io Trusted Publisher identity:

- GitHub owner: `kazuk`
- Repository: `sql-fun`
- Workflow filename: `release-publish.yml`
- GitHub environment: none

The old `crates-io-publish-trial.yml` identity does not authorize this workflow.
This account setup must be performed by the crate owner through crates.io.
The workflow uses `rust-lang/crates-io-auth-action@v1` with `id-token: write`
to obtain a temporary publication token. No long-lived token is required for
normal operation.

Trusted Publishing cannot create a nonexistent crate. For that one-time case,
configure `CRATES_IO_BOOTSTRAP_TOKEN` as a repository secret. The publisher uses
it only after confirming the crate is absent. An existing crate always uses
OIDC for missing versions. After bootstrap, configure the crate's production
Trusted Publisher, enable its Trusted Publishing requirement, and remove the
bootstrap secret. Missing bootstrap credentials fail with the crate's name.

The publication job grants `contents: read` to check out source,
`id-token: write` for crates.io authentication, and `packages: read` for the
pinned CI image pull. It has no Pages or repository-write permission.

`.local/bin/publish-crates` retains the #300/#302-proven Cargo-derived dependency
order and exact crate/version skip behavior. Registry errors fail the job;
there is no propagation retry loop. Cargo packages and verifies source during
`cargo publish`; normal-CI `.crate` files are not inputs. No release binary,
Debian package, or manual is rebuilt by this workflow.

## Pages setup and manual input

Select GitHub Actions as the repository's Pages publishing source. Configure
`github-pages` environment protection rules to allow the release tag refs used
by the workflow, rather than allowing only `main`. If environment approvals are
configured, maintainers must satisfy them for deployment.

The Pages job runs after successful crates.io distribution. It downloads
`sql-fun_vX.Y.Z_user-manual.tar.gz` from the triggering Release and extracts the
mdBook output. The archive must contain the built site's `index.html` at its
root, alongside its normal assets; #328 is the producer of this layout.
A missing archive or root index fails. There is no direct mdBook build fallback.
The extracted HTML is passed unchanged to `actions/upload-pages-artifact@v4`
and deployed using `actions/deploy-pages@v4`.

The job grants `contents: read` for source and Release access, `pages: write`
for deployment, and `id-token: write` for Pages authentication. The environment
is `github-pages`; its URL is the deployment action's output. Platform setup
follows [GitHub's custom Pages workflow requirements](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages).

Pages jobs are serialized without cancelling active deployments. Immediately
before deployment, the workflow compares the triggering tag with the highest
valid stable product version among published Releases. Older stable reruns skip
deployment; GitHub's editable prerelease/latest flags do not control this choice.
A newer release arriving during an active deployment will deploy through its
own subsequent successful distribution job. Pages retains one latest stable
manual; historical and snapshot manuals remain versioned Release assets.

## Retry and validation

Rerun the original Actions run after resolving a failure. Release-specific
concurrency prevents simultaneous runs for the same tag. Already-present exact
crate/version pairs are skipped and missing pairs continue in dependency order.
If Pages failed after crates.io succeeded, rerunning completes deployment
without republishing existing crate versions. Pages upload/deployment uses an
attempt-specific Actions artifact name so reruns cannot collide with a previous
attempt's immutable upload. Recovery is forward-only: no tags,
Releases, or published crate versions are rolled back.

The `version-materialization` standard check step includes offline publication
contract tests with mocked Cargo, registry, and GitHub responses. It never
publishes crates or deploys Pages. The disposable trial workflow and exploratory
`test-packaging` version-mutation helper have been removed.

Actual `release.published` end-to-end validation is deferred until #259 provides
a real prepared Release. Repository checks cannot prove crates.io account setup,
OIDC issuance, or Pages environment configuration. Verify those prerequisites
before the first real release; record the resulting run and distribution status.
