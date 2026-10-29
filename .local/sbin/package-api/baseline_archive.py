"""Candidate archive serialization and explicit stable-state replacement."""
import argparse
import gzip
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile

from compare import AnalysisError, require
from resources import read_baseline


def write_candidate(descriptor, output):
    """Publish a validated archive without replacing an existing output."""
    read_baseline(descriptor)
    require(output.is_absolute() and not output.exists(), 'new absolute candidate output required')
    with tempfile.TemporaryDirectory(prefix='.semver-', dir=output.parent) as directory:
        archive_path = Path(directory) / 'candidate.tar.gz'
        with archive_path.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as archive:
                for path in sorted(descriptor.parent.rglob('*')):
                    if not path.is_file():
                        continue
                    info = archive.gettarinfo(path, arcname=str(path.relative_to(descriptor.parent)))
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    info.mode = 0o644
                    with path.open('rb') as source:
                        archive.addfile(info, source)
        # Linking is atomic and fails if an output already exists, including a symlink.
        os.link(archive_path, output)


def adopt(candidate, active, release_kind):
    """The caller chooses release identity; snapshots never touch baseline data."""
    require(release_kind in ('stable', 'snapshot', 'prerelease'), 'explicit release kind required')
    require(candidate.is_absolute() and active.is_absolute(), 'absolute candidate and active paths required')
    if release_kind != 'stable':
        return dict(status='unchanged', release_kind=release_kind)
    require(not active.is_symlink() and active.is_file(), 'existing regular active baseline required')
    require(not candidate.is_symlink() and candidate.is_file(), 'regular selected candidate required')
    from lifecycle import unpack_baseline
    with tempfile.TemporaryDirectory(prefix='.semver-', dir=active.parent) as directory:
        scratch = Path(directory)
        # Freeze the selected bytes so validation and replacement consume the same file.
        selected = scratch / 'selected.tar.gz'
        shutil.copyfile(candidate, selected)
        old = unpack_baseline(active, scratch / 'active')
        new = unpack_baseline(selected, scratch / 'candidate')
        old_state, _ = read_baseline(old)
        new_state, _ = read_baseline(new)
        require(old_state['target'] == new_state['target'], 'baseline/candidate targets differ')
        source = new_state.get('source')
        require(isinstance(source, dict) and source.get('mode') == 'commit' and source.get('dirty') is False,
                'stable adoption requires a committed candidate')
        os.replace(selected, active)
    return dict(status='adopted', release_kind=release_kind, active=str(active))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--active', required=True, type=Path)
    parser.add_argument('--release-kind', required=True, choices=('stable', 'snapshot', 'prerelease'))
    args = parser.parse_args()
    try:
        result = adopt(args.candidate, args.active, args.release_kind)
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, EOFError, tarfile.TarError) as error:
        print(json.dumps(dict(status='adoption-error', diagnostics=str(error))))
        return 1
    print(json.dumps(result))
    return 0
