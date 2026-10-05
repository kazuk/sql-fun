"""Select release inputs, materialize packages, and validate baseline layouts."""
import json
from pathlib import Path
import shutil
import tempfile

from . import commands, storage
from .model import CheckError, LEVELS, SCHEMA, crate_descriptor, validate_crate_descriptor


def metadata(source):
    data = json.loads(commands.release_metadata(source))
    members = set(data["workspace_members"])
    packages = sorted((p for p in data["packages"] if p["id"] in members
                       and p["source"] is None and p["publish"] != []), key=lambda p: p["name"])
    if not packages:
        raise CheckError("no publishable workspace crates")
    intent = (data["metadata"] or {}).get("next-release", {})
    if not isinstance(intent, dict):
        raise CheckError("workspace.metadata.next-release must be a table")
    for package in packages:
        name = package["name"]
        if intent.get(name) not in LEVELS:
            raise CheckError(f"missing or invalid workspace.metadata.next-release for {name}")
    return packages, intent


def repository_files(source):
    # --source authorizes reading the selected repository, including local edits.
    names = commands.git(source, "ls-files", "-z", "--cached", "--others", "--exclude-standard")
    return sorted({name for name in names.split("\0") if name
                   and name != ".local/semver_baseline.tar.gz"})


def repository_support_file(name):
    """Repository operations and documentation do not compile into crate APIs."""
    directories = (".github/", ".local/", ".codex/", "book/", "docs/", "stats/", "ci/", "graphify-out/")
    return name.startswith(directories) or name in {".gitignore", ".gitattributes", "LICENSE"}


def package_inventory(source, packages, repository, cache):
    """Refresh Cargo's file selection only when membership inputs change."""
    selection_files = [name for name in repository
                       if Path(name).name in {"Cargo.toml", ".gitignore"}]
    selection_digest = storage.source_key(source, selection_files, "", [], "inventory-v1")
    key = storage.digest_value([str(source), repository, selection_digest])
    inventory = cache / ("inventory-" + key + ".json")
    if not inventory.is_file():
        selected = {package["name"]: commands.package_files(source, package) for package in packages}
        storage.write_json(inventory, selected)
    return storage.read_json(inventory)


def api_input_files(source, packages, repository, inventory):
    """Cargo-selected package inputs, excluding API-irrelevant Markdown content.

    Candidate archives still use every selected repository input. The local API
    cache tracks non-Markdown package inputs plus release/build configuration.
    """
    inputs = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
              ".local/bin/materialize-release-versions", ".local/bin/bump-version"}
    for package in packages:
        directory = Path(package["manifest_path"]).parent
        has_build_script = any("custom-build" in target["kind"] for target in package["targets"])
        entrypoints = {Path(target["src_path"]) for target in package["targets"]}
        for name in inventory[package["name"]]:
            path = directory / name
            relative = str(path.relative_to(source))
            rust_source = path.suffix == ".rs" or path in entrypoints
            documentation = path.suffix.lower() == ".md" or repository_support_file(relative)
            if path.is_file() and (has_build_script or rust_source or not documentation):
                inputs.add(relative)
    inputs.update(name for name in repository if name.startswith(".cargo/"))
    return sorted(inputs)


def selected_source(source, cache):
    if Path(commands.git(source, "rev-parse", "--show-toplevel").strip()).resolve() != source:
        raise CheckError("--source must select the repository root")
    revision = commands.git(source, "rev-parse", "HEAD").strip()
    files = [name for name in repository_files(source)
             if not (source / name).is_relative_to(cache)]
    return revision, files


def copy_selected_checkout(source, files, destination):
    destination.mkdir()
    for name in files:
        original = source / name
        if original.is_file() or original.is_symlink():
            copied = destination / name
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(original, copied, follow_symlinks=False)


def extract_release_packages(packages, archives, destination):
    destination.mkdir()
    for package in packages:
        name, version = package["name"], package["version"]
        storage.unpack(archives / f"{name}-{version}.crate", destination)
        (destination / f"{name}-{version}").rename(destination / name)
    return [crate_descriptor(package) for package in packages]


def connect_packaged_dependencies(packaged, entries, checkout):
    # Reconnect published contents, never original sources or registry versions
    # of workspace crates. Cargo has already normalized the package manifests.
    members = [entry["name"] for entry in entries]
    patches = "\n".join(f'{json.dumps(name)} = {{ path = {json.dumps(name)} }}' for name in members)
    (packaged / "Cargo.toml").write_text(
        '[workspace]\nresolver = "2"\nmembers = ' + json.dumps(members) +
        '\n\n[patch.crates-io]\n' + patches + '\n')
    if (checkout / "Cargo.lock").is_file():
        shutil.copy2(checkout / "Cargo.lock", packaged / "Cargo.lock")


def build_packaged_snapshot(source, files, cache, destination, descriptor):
    with tempfile.TemporaryDirectory(dir=cache) as temporary:
        stage = Path(temporary)
        checkout, build = stage / "checkout", stage / "build"
        copy_selected_checkout(source, files, checkout)
        commands.materialize_release(checkout)
        materialized, _ = metadata(checkout)
        commands.package_release(checkout, materialized, build)
        packaged = stage / "packages"
        descriptor["crates"] = extract_release_packages(materialized, build / "package", packaged)
        connect_packaged_dependencies(packaged, descriptor["crates"], checkout)
        storage.write_manifest(stage, descriptor)
        shutil.rmtree(checkout)
        shutil.rmtree(build)
        stage.rename(destination)


def prepare(source, cache, target, features, *, candidate=False):
    revision, files = selected_source(source, cache)
    packages, intent = metadata(source)
    selected = files
    if not candidate:
        inventory = package_inventory(source, packages, files, cache)
        selected = api_input_files(source, packages, files, inventory)
    policy = "candidate-v1" if candidate else "local-api-v1"
    key = storage.source_key(source, selected, target, features, policy)
    root = cache / ("source-" + key)
    if not (root / "manifest.json").is_file():
        descriptor = {"schema": SCHEMA, "source_revision": revision,
                      "source_digest": key, "target": target, "features": features}
        build_packaged_snapshot(source, files, cache, root, descriptor)
    manifest = storage.read_manifest(root)
    if manifest["source_revision"] != revision:
        manifest["source_revision"] = revision
        storage.write_manifest(root, manifest)
    return root, manifest, intent


def baseline_root(archive, cache, target, features):
    if not archive.is_file():
        raise CheckError(f"active baseline missing: {archive}; generate from an explicitly approved revision with the candidate command")
    key = storage.file_digest(archive)
    root = cache / ("baseline-" + key)
    if not root.is_dir():
        with tempfile.TemporaryDirectory(dir=cache) as temporary:
            stage = Path(temporary)
            storage.unpack(archive, stage)
            validate_manifest(stage, target, features)
            stage.rename(root)
    return root, validate_manifest(root, target, features)


def validate_manifest(root, target, features):
    manifest = storage.read_manifest(root)
    if not isinstance(manifest, dict):
        raise CheckError("baseline descriptor must be an object")
    if manifest.get("schema") != SCHEMA:
        raise CheckError("unsupported baseline schema")
    if manifest.get("target") != target or manifest.get("features") != features:
        raise CheckError("baseline target/features differ from explicitly requested analysis scope")
    crates = manifest.get("crates")
    if not isinstance(crates, list) or not crates:
        raise CheckError("baseline has no crates")
    names = set()
    for entry in crates:
        validate_crate_descriptor(entry)
        name = entry["name"]
        if name in names or not (root / "packages" / name / "Cargo.toml").is_file():
            raise CheckError(f"duplicate or missing baseline package: {name}")
        names.add(name)
    return manifest
