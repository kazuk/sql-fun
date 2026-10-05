"""Intent-revealing adapters for repository and analysis tools."""
import subprocess
import sys

from .model import CheckError


def run(args, cwd=None, capture=False):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=capture)
    if result.returncode:
        if capture:
            print(result.stderr, file=sys.stderr, end="")
        raise CheckError(f"command failed ({result.returncode}): {' '.join(map(str, args))}")
    return result.stdout if capture else None


def git(source, *arguments, capture=True):
    return run(["git", *arguments], source, capture)


def cargo(source, *arguments, capture=False):
    return run(["cargo", *arguments], source, capture)


def release_metadata(source):
    return cargo(source, "metadata", "--no-deps", "--format-version=1", capture=True)


def materialize_release(source):
    git(source, "init", "-q", capture=False)
    run(["bash", str(source / ".local/bin/materialize-release-versions"), "--release"], source)


def package_release(source, packages, output):
    arguments = ["package", "--allow-dirty", "--no-verify", "--target-dir", str(output)]
    for package in packages:
        arguments.extend(["--package", package["name"]])
    cargo(source, *arguments)


def package_files(source, package):
    return cargo(source, "package", "--list", "--offline", "--allow-dirty", "--no-verify",
                 "--package", package["name"], capture=True).splitlines()


def require_semver_version(expected):
    actual = cargo(None, "semver-checks", "--version", capture=True).strip()
    if actual != "cargo-semver-checks " + expected:
        raise CheckError(f"CI requires cargo-semver-checks {expected}, got {actual}")


def semver_probe(current_json, baseline_json, release_level):
    arguments = ["cargo", "semver-checks", "--current-rustdoc", str(current_json),
                 "--baseline-rustdoc", str(baseline_json), "--color", "never",
                 "--release-type", release_level]
    return subprocess.run(arguments).returncode


def rustdoc_toolchain(directory, environment):
    compiler = run([environment.get("RUSTC", "rustc"), "-Vv"], directory, capture=True)
    rustdoc = run([environment.get("RUSTDOC", "rustdoc"), "--version"], directory, capture=True)
    return compiler, rustdoc


def generate_rustdoc(arguments, directory, environment, crate):
    result = subprocess.run(arguments, cwd=directory, env=environment)
    if result.returncode:
        raise CheckError(f"rustdoc generation failed for {crate}")
