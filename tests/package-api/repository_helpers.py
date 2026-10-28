"""Small domain fixtures for source capture and release-intent contracts."""
from pathlib import Path
import subprocess


def git(root, *args):
    return subprocess.check_output(['git', '-c', 'core.hooksPath=/dev/null', *args], cwd=root,
                                   text=True, stderr=subprocess.DEVNULL).strip()


def source_repository(root, explicit_includes=True):
    root.mkdir()
    git(root, 'init', '-q')
    git(root, 'config', 'user.email', 'fixture@example.invalid')
    git(root, 'config', 'user.name', 'Fixture')
    (root / '.gitignore').write_text('target/\nignored.rs\nstaged_ignored.rs\nremoved_from_index.rs\n')
    (root / 'Cargo.toml').write_text('[package]\nname="probe"\nversion="1.0.0"\n'
                                   + ('include=["Cargo.toml", "src/**", "ignored.rs"]\n' if explicit_includes else ''))
    (root / 'src').mkdir()
    (root / 'src/lib.rs').write_text('pub fn initial() {}\n')
    (root / 'src/deleted.rs').write_text('// tracked file\n')
    (root / 'src/removed_from_index.rs').write_text('// initially tracked despite ignore rules\n')
    git(root, 'add', '.')
    git(root, 'add', '-f', 'src/removed_from_index.rs')
    git(root, 'commit', '-qm', 'test: initial source')
    return git(root, 'rev-parse', 'HEAD')


def dirty_source(root):
    (root / 'src/lib.rs').write_text('pub fn staged() {}\n')
    git(root, 'add', 'src/lib.rs')
    (root / 'src/lib.rs').write_text('pub fn unstaged() {}\n')
    (root / 'src/staged_ignored.rs').write_text('pub fn staged_ignored() {}\n')
    git(root, 'add', '-f', 'src/staged_ignored.rs')
    git(root, 'rm', '--cached', 'src/removed_from_index.rs')
    (root / 'src/deleted.rs').unlink()
    (root / 'src/untracked.rs').write_text('pub fn untracked() {}\n')
    (root / 'ignored.rs').write_text('pub fn explicitly_included() {}\n')
    (root / 'excluded.txt').write_text('not in Cargo include list\n')
    (root / 'target').mkdir()
    (root / 'target/build-output').write_text('not source\n')


def source_fingerprint(root):
    return dict(files={str(p.relative_to(root)): p.read_bytes() for p in root.rglob('*')
                       if p.is_file() and '.git' not in p.relative_to(root).parts},
                index=(root / '.git/index').read_bytes())
