# Release distribution

This guide is for maintainers publishing a prepared sql-fun product release.
Issue #259 prepares the immutable tag, materialized source, draft GitHub Release,
and its assets. A maintainer publishes the draft as the final approval gate.
`.github/workflows/release-publish.yml` then handles distribution (#260):

1. Validate the product release tag and resolve its immutable source commit.
2. Publish all publishable workspace crates to crates.io.
3. For stable releases, merge the entire originating release branch into `main`.
4. Deploy the existing manual to Pages for the latest stable release only, after
   that merge succeeds.

Announcements, when configured separately, must follow successful distribution.
The GitHub Release can be visible before crates.io or Pages completes.

## Release identity and source

`release.tag_name` is authoritative. The shared
`.local/bin/parse-release-version` parser accepts `vX.Y.Z`,
`vX.Y.Z-snapshot`, and `vX.Y.Z-snapshotN` (where N is a sequence of digits).
Numeric version components must not have leading zeros. Other prerelease names,
build metadata, and invalid tags fail before distribution. The upstream release
branch suffix must equal this product tag. The merge job derives
`refs/heads/release/<tag>` from that validated identity.

Classification does not use `release.target_commitish`, `release.prerelease`,
or a workflow input. Snapshot releases never enter the merge or Pages jobs and
never obtain a repository-write credential.

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

## Stable release merge and repository authorization

After crates.io succeeds, the stable-only merge job checks the remote tag and
`release/<tag>` branch against the recorded immutable source `R`. The branch tip
must equal `R`; missing, advanced, rewound, or mismatched branches fail. A tag
that no longer resolves to `R` also fails. Keep the release branch available and
unchanged for this handoff and subsequent reruns.

The job uses the existing maintenance App through
`actions/create-github-app-token@v3`, with `MAINTENANCE_BOT_CLIENT_ID` and
`MAINTENANCE_BOT_PRIVATE_KEY`. Its installation token is restricted to this
repository, with contents write and workflows write: the complete release
history may include workflow changes. It receives no crates.io/OIDC, Pages,
Actions, or pull-request permission. The job's separate `GITHUB_TOKEN` remains
contents read.

The existing active `main` ruleset (`11504670`) authorizes Integration `5016922`
with an always exception. The job verifies the token action's App identity and
that existing exception before checkout with write credentials. This is an
explicit reliance on existing repository policy, including its exception to
PR/check requirements; granting contents write to `GITHUB_TOKEN` alone does not
provide that authorization. Legacy branch protection must also permit the
update. The workflow does not change protections or add exceptions. If App
identity, installation permissions, or repository policy differs, stop and
resolve that configuration explicitly; do not substitute another credential.

`.local/sbin/merge-release-branch` checks ancestry before writing. When `main`
already contains `R`, the job succeeds without creating a commit or pushing.
Otherwise it requires one unambiguous common ancestor and makes a normal merge
commit retaining the complete release history, even when a fast-forward would
be possible. Independent changes on `main` are allowed when the merge is clean.
It never cherry-picks, squashes, rebases, force-pushes, resets shared history,
or moves/deletes release tags or branches.

Immediately before a normal push, the helper rechecks the release branch, tag
object, and `main` against the fetched state. Ref movement, merge conflicts,
racing incompatible pushes, and branch-protection rejection fail the job and
prevent Pages deployment in that run. Remote source-ref rechecks and the push
are separate Git operations; the merge always uses pinned `R`, never a later
branch tip. The producer/operator must preserve the immutable release handoff.
An external update to `main` racing after the recheck is still subject to Git's
normal non-fast-forward rejection. The job records source/base/result revisions
and its merge outcome in the Actions summary.

Inspect conflicts or ref movement before rerunning the original run. Do not
rewrite release history to make a failed attempt disappear. If a push succeeded
but its response or a later step failed, the rerun recognizes remote ancestry
and makes no duplicate merge commit. A failed push is not automatically retried
against a newly selected base. A rejected or ambiguous attempt remains visible
for explicit operator action.

## Pages setup and manual input

Select GitHub Actions as the repository's Pages publishing source. Configure
`github-pages` environment protection rules to allow the release tag refs used
by the workflow, rather than allowing only `main`. If environment approvals are
configured, maintainers must satisfy them for deployment.

The Pages job runs after successful crates.io distribution and stable-branch
merge. A failed merge blocks Pages; snapshots run neither job. It downloads
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
If the merge or Pages failed after crates.io succeeded, rerunning skips existing
crate versions, validates the same branch/tag handoff, and skips the merge when
its history is already incorporated before resuming Pages. Pages upload/deployment
uses an attempt-specific Actions artifact name so reruns cannot collide with a
previous attempt's immutable upload. Recovery is forward-only: no tags,
Releases, or published crate versions are rolled back.

Offline release publication and preparation fixtures are excluded from standard
local checks and CI because no recurring execution budget is approved.

Actual `release.published` end-to-end validation is deferred until #259 provides
a real prepared Release. Repository checks cannot prove crates.io account setup,
OIDC issuance, or Pages environment configuration. Verify those prerequisites
before the first real release; record the resulting run and distribution status.
