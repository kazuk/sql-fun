"""Archive, atomic replacement, cache locking, and content fingerprints."""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import tomllib

from .model import SCHEMA


def source_key(source, files, target, features, policy):
    digest = hashlib.sha256(json.dumps([SCHEMA, target, features, policy]).encode())
    for name in files:
        path = source / name
        digest.update(os.fsencode(name) + b"\0")
        if path.is_symlink():
            digest.update(b"L" + os.fsencode(os.readlink(path)))
        elif path.is_file():
            digest.update(b"F" + str(path.stat().st_mode & 0o777).encode() + b"\0" + path.read_bytes())
        else:
            digest.update(b"D")
        # Deleted files and submodule gitlinks are not package source inputs.
    return digest.hexdigest()


def unpack(archive, destination):
    with tarfile.open(archive, "r:gz") as bundle:
        bundle.extractall(destination, filter="data")


def digest_value(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def file_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def read_json(path):
    return json.loads(path.read_text())


def write_json(path, value):
    with atomic_destination(path) as temporary:
        temporary.write_text(json.dumps(value, indent=2) + "\n")


def read_manifest(root):
    return read_json(root / "manifest.json")


def write_manifest(root, manifest):
    write_json(root / "manifest.json", manifest)


@contextmanager
def locked_cache(cache):
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


@contextmanager
def atomic_destination(destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as output:
        temporary = Path(output.name)
    try:
        yield temporary
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def write_candidate(root, destination):
    with atomic_destination(destination) as temporary:
        with tarfile.open(temporary, "w:gz") as archive:
            archive.add(root / "manifest.json", arcname="manifest.json")
            archive.add(root / "packages", arcname="packages")


def atomic_copy(source, destination):
    with atomic_destination(destination) as temporary:
        shutil.copyfile(source, temporary)


def adopt_candidate(candidate, destination):
    atomic_copy(candidate, destination)


def cargo_configuration(directory):
    # These are Cargo's documented configuration inputs for this invocation.
    directories = [parent / ".cargo" for parent in [directory, *directory.parents]]
    directories.append(Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))))
    digest = hashlib.sha256()
    reusable = True
    for config_directory in directories:
        for name in ("config", "config.toml"):
            path = config_directory / name
            if path.is_file():
                content = path.read_bytes()
                digest.update(os.fsencode(path) + b"\0" + content)
                # Included configurations can have arbitrary transitive inputs.
                # Let Cargo resolve them each time rather than cache partial inputs.
                reusable &= "include" not in tomllib.loads(content.decode())
    return digest.hexdigest(), reusable
