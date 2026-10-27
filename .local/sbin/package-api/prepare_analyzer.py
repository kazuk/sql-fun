"""Build the pinned analyzer with the #826 trait-presence query correction.

Internal preparation behind the standard test flow. Upstream source is verified
in temporary storage; no installed contributor tool is replaced.
"""
import fcntl
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

VERSION = "0.51.0-sqlfun.1"
ARCHIVE_SHA256 = "484e65e62077aed1a119d04ab414038d20b9013302b7353487ff69a461842bf8"
SOURCE = "https://static.crates.io/crates/cargo-semver-checks/cargo-semver-checks-0.51.0.crate"


def replace_once(path, before, after):
    source = path.read_text()
    if source.count(before) != 1:
        raise ValueError(f"unexpected pinned analyzer source: {path}")
    path.write_text(source.replace(before, after))


def correct_trait_query(source):
    # Reuse trait_missing's existing anti-join: a kind change requires the
    # trait to have disappeared, even when a value shares its import path.
    query = source / "src/lints/trait_changed_kind.ron"
    replace_once(query, '            current {\n', '''            current {
                item @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
                    ... on Trait {
                        visibility_limit @filter(op: "=", value: ["$public"])
                        importable_path {
                            path @filter(op: "=", value: ["%path"])
                            public_api @filter(op: "=", value: ["$true"])
                        }
                    }
                }
''')
    replace_once(query, '        "public": "public",', '        "public": "public",\n        "zero": 0,')


def prepare(root):
    destination = root / "target/package-api-tools" / VERSION
    destination.mkdir(parents=True, exist_ok=True)
    # Coverage and package contracts may prepare tools in separate nextest
    # processes. Serialize installation and recheck the cached executable.
    with (destination / "prepare.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return install_analyzer(destination)


def install_analyzer(destination):
    executable = destination / "bin/cargo-semver-checks"
    if executable.is_file():
        result = subprocess.run([str(executable), "--version"], check=True, text=True, capture_output=True)
        if result.stdout.strip() == f"cargo-semver-checks {VERSION}":
            return executable
        raise ValueError("prepared analyzer has an unexpected identity")
    with tempfile.TemporaryDirectory(prefix="semver-source-") as directory:
        scratch = Path(directory)
        archive = scratch / "source.crate"
        with urllib.request.urlopen(SOURCE) as response:
            contents = response.read()
        if hashlib.sha256(contents).hexdigest() != ARCHIVE_SHA256:
            raise ValueError("pinned analyzer source checksum mismatch")
        archive.write_bytes(contents)
        with tarfile.open(archive) as package:
            package.extractall(scratch, filter="data")
        source = scratch / "cargo-semver-checks-0.51.0"
        correct_trait_query(source)
        # Keep upstream package/dependency identities and diagnostic links intact.
        # Only the executable's advertised CLI identity distinguishes this build.
        replace_once(source / "src/main.rs", '#[command(version, propagate_version = true)]',
                     f'#[command(version = "{VERSION}", propagate_version = true)]')
        subprocess.run(["cargo", "install", "--path", str(source), "--locked", "--root", str(destination),
                        "--target-dir", str(destination / "build")],
                       cwd=source, check=True, stdout=sys.stderr)
    return executable


if __name__ == "__main__":
    print(prepare(Path(__file__).resolve().parents[3]))
