# Feature set operations when adding Cargo features

This rule is for contributors adding Cargo features and maintainers reviewing
feature coverage or adopting a stable-release baseline. It describes how to
add coverage without treating an unreleased feature as an existing public API.
See [SemVer release intent validation](semver-intent.md) for tool prerequisites,
the comparison contract, and candidate provenance requirements.

## Preserve released configurations

The active `.local/semver_baseline.tar.gz` defines the configurations whose APIs
have already entered compatibility checking. Preserve every baseline feature
set's name and per-crate mapping in `.local/semver-feature-sets.toml`. Adding a
feature to an existing set changes that mapping and is rejected; it is not the
operation for introducing an unreleased feature.

A new feature has no previous users at introduction, but existing configurations
still have their previous APIs. Keep their checks active. Introducing a feature
must not erase their baseline, rename their sets to bypass comparison, or raise
release intent just to accommodate the new configuration. Existing API changes
remain subject to the declared release intent.

## Contributor procedure

1. Add the Cargo feature and its implementation. Keep it opt-in when its API is
   intended to be absent from existing configurations. Enabling it by default
   also changes those configurations because this policy enables default Cargo
   features; that change needs the usual API review and comparison.
2. Decide explicitly which supported configuration exercises the new feature.
   If it needs SemVer coverage, add a **new named feature set** to
   `.local/semver-feature-sets.toml` in the same PR. Leave baseline sets intact.
   Map every publishable crate, including binary-only crates. Copy the chosen
   configuration's full mapping and add the feature only to crates that expose
   it, including any required forwarding features. Use `[]` explicitly where
   no additional feature is selected. Do not use `--all-features`.
3. Describe the new set's purpose and coverage in the PR. A feature intentionally
   outside supported sets needs an explicit coverage limitation and separate
   tests; adding a Cargo feature alone does not add SemVer coverage.
4. Run the SemVer step with the analysis tool installed:

   ```sh
   .local/bin/check-test-flow --step=semver --require-semver
   ```

   Verify that existing sets pass against the unchanged active baseline and the
   new set reports `new configuration (no previous API)`. That report is expected:
   the new configuration has no released API to compare. Build and test the new
   feature separately; the absence of a historical API comparison does not test
   its correctness. Verify the PR's SemVer CI result as well.
5. Keep the active baseline unchanged in the feature PR. Normal CI generates a
   candidate containing all current sets, including the new one. Generating or
   downloading that candidate does not activate it.

For example, when adding an opt-in `new-capability` feature to `sql-fun-core`,
retain `postgres` and `sqlite` exactly as recorded in the baseline. Add a set
such as `postgres-new-capability` with the full `postgres` mapping, changing
only `sql-fun-core = []` to `sql-fun-core = ["new-capability"]`, plus any explicitly
required forwarding features. This example assumes that feature exists in the
selected crate; it is not a feature currently provided by the repository.

Until stable adoption, sets absent from the active baseline continue to report
no previous API. Their feature APIs may evolve during development without a
historical SemVer violation. Keep already released sets unchanged and passing
throughout that development.

## Stable release and subsequent changes

1. Complete the stable release using the candidate from successful required
   normal CI for the exact release target commit. Verify that the candidate
   records the intended feature sets and complete per-crate mappings.
2. After stable release completion, adopt that candidate into
   `.local/semver_baseline.tar.gz` in the release-state back-commit, following
   the [stable release adoption contract](semver-intent.md#stable-release-adoption-contract-259).
   Snapshot releases leave the active baseline unchanged.
3. Subsequent checks compare the newly adopted set normally. Its API now has a
   released baseline; removing or changing it can require greater release
   intent, just like other released configurations.

Adding another unreleased configuration repeats the contributor procedure.
Removing an adopted set or changing its feature mapping requires an explicitly
reviewed baseline transition. Do not replace the active archive with a current
development candidate to make a failing check pass. A new set also does not
retroactively cover an already released feature: document that coverage gap and
review an explicit baseline transition rather than claiming there were no users.
