"""Script-owned package preparation, analyzer execution and policy combination."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib

RUSTC = "rustc 1.99.1 (58cf8f9c3 2026-10-13)"
CARGO = "cargo 1.99.1 (5f94df478 2026-08-27)"
RUSTDOC = "rustdoc 1.99.1 (58cf8f9c3 2026-10-13)"
LIMITS = [
    "Structural analysis does not prove compatibility of runtime behavior, macro expansion or accepted macro input.",
    "An empty diff does not resolve a known behavioral compatibility concern; complementary review/tests are required.",
    "One comparison does not establish publication history or complete released configuration coverage (#358).",
]


class AnalysisError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise AnalysisError(message)


def run(command, cwd, env, allowed=(0,)):
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True)
    require(result.returncode in allowed,
            f"command failed ({result.returncode}): {command!r}\n{result.stdout}\n{result.stderr}")
    return result


def explicit_path(value):
    require(isinstance(value, str) and Path(value).is_absolute(), "paths must be explicit absolute paths")
    return Path(value).resolve(strict=True)


def validate_side(side):
    require(isinstance(side, dict), "comparison side must be an object")
    require(set(side) == {"package", "dependencies", "features", "default_features"}, "invalid side fields")
    require(type(side["default_features"]) is bool, "default_features must be an explicit boolean")
    require(isinstance(side["features"], list) and
            all(isinstance(f, str) and f and not re.search(r"[\s,]", f) for f in side["features"]),
            "features must be an explicit list of feature names")
    require(isinstance(side["dependencies"], list), "dependencies must be an explicit list of package directories")
    return [explicit_path(side["package"])] + [explicit_path(p) for p in side["dependencies"]]


def tool_environment(toolchain, scratch):
    # Do not inherit compiler wrappers/flags, target selection or repository Cargo
    # configuration. Cargo's registry/cache home remains available for offline deps.
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("CARGO_", "RUST"))}
    if "CARGO_HOME" in os.environ:
        env["CARGO_HOME"] = os.environ["CARGO_HOME"]
    if "RUSTUP_HOME" in os.environ:
        env["RUSTUP_HOME"] = os.environ["RUSTUP_HOME"]
    env.update(RUSTUP_TOOLCHAIN=toolchain, RUSTC_BOOTSTRAP="1", CARGO_TERM_COLOR="never",
               CARGO_ENCODED_RUSTFLAGS="", CARGO_ENCODED_RUSTDOCFLAGS="",
               CARGO_BUILD_RUSTC_WRAPPER="", CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER="")
    # Explicit executable selection also takes precedence over Cargo home settings.
    for name in ("rustc", "rustdoc"):
        path = run(["rustup", "which", "--toolchain", toolchain, name], scratch, env).stdout.strip()
        env[name.upper()] = path
    return env


def prepare_side(paths, root):
    root.mkdir()
    packages = []
    for index, source in enumerate(paths):
        require(source.is_dir(), f"not a package directory: {source}")
        # Inputs are package contents, not arbitrary worktrees; reject links rather
        # than inspecting user assets outside the explicitly supplied directories.
        require(not any(p.is_symlink() for p in source.rglob("*")), f"package contains symlinks: {source}")
        destination = root / f"package-{index}"
        shutil.copytree(source, destination)
        manifest = tomllib.loads((destination / "Cargo.toml").read_text())
        require("package" in manifest, f"missing package identity: {source}")
        package = manifest["package"]
        require(isinstance(package, dict), "package manifest table is invalid")
        require(isinstance(package.get("name"), str) and isinstance(package.get("version"), str),
                "use self-contained packaged manifests, without workspace inheritance")
        require(not manifest.get("workspace") and not manifest.get("patch") and not manifest.get("replace"),
                "input must be one package, not a source workspace")
        # Cargo source packages normalize local dependency paths to registry
        # requirements. Never follow paths retained in a source-tree manifest.
        tables = [manifest] + list(manifest.get("target", {}).values())
        for table in tables:
            for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
                for dep in table.get(kind, {}).values():
                    require(not isinstance(dep, dict) or not ({"path", "workspace"} & dep.keys()),
                            "use packaged dependency manifests without source path/workspace dependencies")
        packages.append((package, destination))
    names = [p["name"] for p, _ in packages]
    require(len(names) == len(set(names)), "duplicate supplied package name")
    config = root / ".cargo"
    config.mkdir()
    patches = ["[patch.crates-io]"]
    for package, path in packages[1:]:
        patches.append(f'{json.dumps(package["name"])} = {{ path = {json.dumps(str(path))} }}')
    # Force a predictable profile and avoid Cargo-home flags/rustdoc settings.
    patches += ['[build]', 'rustflags = []', 'rustdocflags = []',
                '[profile.dev]', 'debug = 0', 'incremental = false']
    (config / "config.toml").write_text("\n".join(patches) + "\n")
    return packages


def generate(side, paths, root, target, host, env):
    packages = prepare_side(paths, root)
    package, directory = packages[0]
    common = ["--manifest-path", str(directory / "Cargo.toml"), "--offline"]
    if not side["default_features"]:
        common += ["--no-default-features"]
    if side["features"]:
        common += ["--features", ",".join(side["features"])]
    metadata = json.loads(run(["cargo", "metadata", "--format-version", "1", "--filter-platform", target, *common], root, env).stdout)
    selected = next(p for p in metadata["packages"] if Path(p["manifest_path"]).parent == directory)
    require(set(side["features"]) <= selected["features"].keys(), "requested package feature is missing")
    libraries = [t for t in selected["targets"] if "lib" in t["kind"] or "proc-macro" in t["kind"]]
    require(len(libraries) == 1, "exactly one library or proc-macro target is required")
    resolved = {p["manifest_path"]: p["version"] for p in metadata["packages"]}
    for dep, path in packages[1:]:
        require(resolved.get(str(path / "Cargo.toml")) == dep["version"],
                f'supplied dependency not resolved from package contents: {dep["name"]}')
    # Cargo may resolve registry dependencies, but no other local source is allowed.
    allowed_manifests = {str(path / "Cargo.toml") for _, path in packages}
    require(all(p["source"] is not None or p["manifest_path"] in allowed_manifests
                for p in metadata["packages"]), "unexpected local source dependency")
    node = next(n for n in metadata["resolve"]["nodes"] if n["id"] == selected["id"])
    build = root / "build"
    run(["cargo", "rustdoc", *common, "--lib", "--target", target,
         "--target-dir", str(build), "--", "-Z", "unstable-options", "--output-format", "json"], root, env)
    # Cargo builds/document proc-macro targets for the host, even with --target.
    output = build if "proc-macro" in libraries[0]["kind"] else build / target
    artifact = output / "doc" / (libraries[0]["name"].replace("-", "_") + ".json")
    documented_target = host if "proc-macro" in libraries[0]["kind"] else target
    validate_json(artifact, libraries[0]["name"], package["version"], documented_target)
    return artifact, {"name": package["name"], "version": package["version"],
                      "target_kind": libraries[0]["kind"], "features": side["features"],
                      "default_features": side["default_features"], "resolved_features": node["features"],
                      "feature_inventory": sorted(selected["features"]), "schema": 61,
                      "documented_target": documented_target}


def validate_json(artifact, name, version, target):
    document = json.loads(artifact.read_text())
    require(isinstance(document, dict), "rustdoc JSON must be an object")
    require(document.get("format_version") == 61, "unsupported rustdoc schema (expected 61)")
    crate = document["index"][str(document["root"])]["name"]
    require(crate == name.replace("-", "_") and document.get("crate_version") == version,
            "unexpected JSON crate identity/version")
    require(document.get("includes_private") is False, "unexpected private-item JSON coverage")
    require(document["target"]["triple"] == target, "unexpected JSON compilation target")


def parse_semver(output, status):
    """Validate 0.51.0 completion/counts before interpreting required levels."""
    require(status in (0, 100), f"SemVer analysis failed with status {status}")
    checked = re.findall(r"^\s*Checked \[[\s\d.]+s\] (\d+) checks: (\d+) pass, (?:(\d+) fail, (\d+) warn, )?(\d+) skip$", output, re.M)
    require(len(checked) == 1, "missing or ambiguous SemVer completion summary")
    require(len(re.findall(r"^\s*Summary ", output, re.M)) == 1 and
            len(re.findall(r"^\s*Checking ", output, re.M)) == 1 and
            len(re.findall(r"^\s*Finished ", output, re.M)) == 1,
            "missing or ambiguous SemVer run boundaries")
    checks, passed, failed, warned, skipped = [int(n or 0) for n in checked[0]]
    require(checks == 229 and checks == passed + failed + warned and skipped == 31,
            "unexpected SemVer check inventory/completion counts")
    failures = re.findall(r"^\s*Summary semver requires new (major|minor) version: (\d+) major and (\d+) minor checks failed$", output, re.M)
    warnings = re.findall(r"^\s*Warning produced (\d+) major and (\d+) minor level warnings$", output, re.M)
    require(len(failures) == (1 if failed else 0) and len(warnings) == (1 if warned else 0),
            "missing or unexpected SemVer finding-level summary")
    require(len(re.findall(r"^\s*Warning ", output, re.M)) == len(warnings),
            "unrecognized SemVer warning summary")
    major = minor = 0
    if failed:
        level, a, b = failures[0]
        major, minor = int(a), int(b)
        require(major + minor == failed and level == ("major" if major else "minor"), "invalid failure levels")
    if warned:
        a, b = map(int, warnings[0])
        require(a + b == warned, "invalid warning levels")
        major += a
        minor += b
    require(status == (100 if failed else 0), "SemVer exit status disagrees with completion")
    require(len(re.findall(r"^--- failure [\w]+:", output, re.M)) == failed and
            len(re.findall(r"^--- warning [\w]+:", output, re.M)) == warned,
            "SemVer finding details disagree with summary")
    if not failed:
        require(len(re.findall(r"^\s*Summary no semver update required$", output, re.M)) == 1,
                "missing SemVer success summary")
    return {"major": major, "minor": minor, "checks": checks, "skipped": skipped,
            "exit_status": status, "diagnostics": output}


def compare(request, wrapper, semver, scratch):
    require(isinstance(request, dict), "comparison request must be an object")
    require(set(request) == {"old", "new", "target", "toolchain"}, "invalid request fields")
    require(isinstance(request["target"], str) and re.fullmatch(r"[\w-]+", request["target"]), "explicit target triple required")
    require(isinstance(request["toolchain"], str) and request["toolchain"], "explicit toolchain required")
    paths = {side: validate_side(request[side]) for side in ("old", "new")}
    env = tool_environment(request["toolchain"], scratch)
    for command, expected in (("rustc", RUSTC), ("cargo", CARGO), ("rustdoc", RUSTDOC)):
        require(run([command, "--version"], scratch, env).stdout.strip() == expected,
                f"unsupported {command} producer; expected {expected}")
    require(run([str(semver), "--version"], scratch, env).stdout.strip() == "cargo-semver-checks 0.51.0",
            "expected cargo-semver-checks 0.51.0")
    host = run(["rustc", "--print", "host-tuple"], scratch, env).stdout.strip()
    artifacts, coverage = {}, {}
    for side in ("old", "new"):
        try:
            artifacts[side], coverage[side] = generate(request[side], paths[side], scratch / side, request["target"], host, env)
        except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, StopIteration) as error:
            raise AnalysisError(f"{side} JSON production: {error}") from error
    require(coverage["old"]["name"] == coverage["new"]["name"], "old/new package names differ")
    diff = json.loads(run([str(wrapper), "public-api-diff", "--old", str(artifacts["old"]),
                           "--new", str(artifacts["new"])], scratch, env).stdout)
    require(set(diff) == {"public_api_version", "added", "removed", "changed"} and
            diff["public_api_version"] == "0.52.0" and
            all(isinstance(diff[k], list) for k in ("added", "removed", "changed")), "invalid diff wrapper output")
    result = run([str(semver), "check-release", "--baseline-rustdoc", str(artifacts["old"]),
                  "--current-rustdoc", str(artifacts["new"]), "--release-type", "patch", "--color", "never"],
                 scratch, env, allowed=(0, 100))
    findings = parse_semver(result.stdout + "\n" + result.stderr, result.returncode)
    removed = sorted(set(coverage["old"]["feature_inventory"]) - set(coverage["new"]["feature_inventory"]))
    impact = "patch"
    if diff["added"] or findings["minor"]:
        impact = "minor"
    if diff["removed"] or diff["changed"] or removed or findings["major"]:
        impact = "major"
    return {"status": "detected", "impact": impact, "diff": diff, "removed_features": removed,
            "semver": findings, "coverage": coverage,
            "producer": {"rustc": RUSTC, "cargo": CARGO, "rustdoc": RUSTDOC, "target": request["target"], "host": host},
            "limits": LIMITS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, help="absolute path to explicit comparison JSON")
    parser.add_argument("--diff-wrapper", required=True, help="absolute path to built xbuild")
    parser.add_argument("--semver-checks", required=True, help="absolute path to cargo-semver-checks 0.51.0")
    args = parser.parse_args()
    try:
        request = json.loads(explicit_path(args.request).read_text())
        wrapper, semver = explicit_path(args.diff_wrapper), explicit_path(args.semver_checks)
        with tempfile.TemporaryDirectory(prefix="package-api-") as directory:
            report = compare(request, wrapper, semver, Path(directory))
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, StopIteration) as error:
        print(json.dumps({"status": "analysis-error", "diagnostics": str(error), "limits": LIMITS}))
        return 1
    print(json.dumps(report))
    return 0
