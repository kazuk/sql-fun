"""Generate cached packaged APIs and compare detected impact with intent."""
import os

from . import commands, storage
from .model import CheckError, LEVELS, exceeds_intent


def api_rustdoc(root, name, target, features, proc_macro=False, lib_name=None, side="current"):
    """Generate the selected packaged library API with a matching build context."""
    # Cargo package timestamps are normalized: distinct source snapshots must
    # not share fingerprints for build-script outputs or path dependencies.
    build = root / ("api-build-" + side)
    args = ["cargo", "rustdoc", "--manifest-path", str(root / "packages" / name / "Cargo.toml"),
            "--package", name, "--lib", "--target", target, "--target-dir", str(build)]
    if features:
        args.extend(["--features", ",".join(features)])
    args.extend(["--", "-Z", "unstable-options", "--output-format", "json", "--document-private-items"])
    environment = dict(os.environ, RUSTC_BOOTSTRAP="1")
    compiler, rustdoc = commands.rustdoc_toolchain(root / "packages", environment)
    context = storage.digest_value(environment)
    configuration, reusable = storage.cargo_configuration(root / "packages")

    def json_path():
        lock = root / "packages" / "Cargo.lock"
        lock_digest = storage.file_digest(lock)
        key = storage.digest_value([compiler, rustdoc, context, configuration, args, lock_digest])
        return root / "api-json" / key / ((lib_name or name.replace("-", "_")) + ".json")

    output = json_path()
    if reusable and output.is_file():
        return output
    commands.generate_rustdoc(args, root / "packages", environment, name)
    # Cargo may resolve the packaged dependency patch map on the first build.
    output = json_path()
    doc_root = build / "doc" if proc_macro else build / target / "doc"
    storage.atomic_copy(doc_root / output.name, output)
    return output


def detected_level(current, baseline, name, target, features, proc_macro=False,
                   lib_name=None, baseline_entry=None):
    old = baseline_entry or {"proc_macro": proc_macro, "lib_name": lib_name}
    current_json = api_rustdoc(current, name, target, features, proc_macro, lib_name)
    baseline_json = api_rustdoc(baseline, name, target, features, old["proc_macro"], old.get("lib_name"), side="baseline")
    # 0.51.0 documents 100 for completed deny-level failures and 101 for errors.
    # Probe thresholds using cached rustdoc; do not classify by parsing diagnostics.
    for level in LEVELS[:2]:
        status = commands.semver_probe(current_json, baseline_json, level)
        if status == 0:
            return level
        if status != 100:
            raise CheckError(f"API analysis failed for {name} (exit {status})")
    return "major"


def compare(current, candidate, baseline, previous, intent):
    old = {crate["name"]: crate for crate in previous["crates"]}
    new = {crate["name"]: crate for crate in candidate["crates"]}
    removed = sorted(old.keys() - new.keys())
    if removed:
        raise CheckError("baseline crates removed from publishable set; explicit baseline transition required: " + ", ".join(removed))
    failed = False
    for name, entry in new.items():
        if name not in old:
            print(f"{name}: declared={intent[name]}, new crate (no previous API)")
            continue
        if entry["library"] and not old[name]["library"]:
            required = "minor"
        elif old[name]["library"] and not entry["library"]:
            required = "major"
        elif not entry["library"]:
            print(f"{name}: declared={intent[name]}, binary-only (CLI compatibility outside API analysis scope)")
            continue
        else:
            required = detected_level(current, baseline, name, candidate["target"], candidate["features"], entry["proc_macro"], entry["lib_name"], old[name])
        exceeds = exceeds_intent(required, intent[name])
        result = "FAIL (exceeds release intent)" if exceeds else "PASS"
        print(f"{name}: declared={intent[name]}, detected required={required}: {result}", flush=True)
        failed |= exceeds
    return 1 if failed else 0
