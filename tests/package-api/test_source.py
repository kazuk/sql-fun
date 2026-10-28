"""Small Git/Cargo-membership source-mode contracts, without builds."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
from repository_helpers import source_repository, dirty_source, source_fingerprint
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError
from repository import capture_source


class Source(unittest.TestCase):
    def test_working_tree_membership_and_commit_isolation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'repository'
            revision = source_repository(source)
            dirty_source(source)
            before = source_fingerprint(source)
            local = root / 'local'
            provenance = capture_source(source, local, None)
            self.assertEqual(provenance, dict(mode='working-tree', revision=revision, dirty=True))
            self.assertEqual((local / 'src/lib.rs').read_text(), 'pub fn unstaged() {}\n')
            self.assertFalse((local / 'src/deleted.rs').exists())
            self.assertTrue((local / 'src/untracked.rs').is_file())
            self.assertTrue((local / 'ignored.rs').is_file())
            # Ask Cargo itself which files the captured source would package.
            completed = subprocess.run(['cargo', 'package', '--list', '--allow-dirty', '--offline'],
                                       cwd=local, text=True, capture_output=True, check=True)
            members = completed.stdout.splitlines()
            self.assertIn('ignored.rs', members)
            self.assertIn('src/untracked.rs', members)
            self.assertNotIn('excluded.txt', members)
            self.assertNotIn('target/build-output', members)
            committed = root / 'committed'
            provenance = capture_source(source, committed, revision)
            self.assertFalse(provenance['dirty'])
            self.assertEqual((committed / 'src/lib.rs').read_text(), 'pub fn initial() {}\n')
            self.assertTrue((committed / 'src/deleted.rs').exists())
            self.assertFalse((committed / 'ignored.rs').exists())
            self.assertFalse((committed / 'src/untracked.rs').exists())
            self.assertEqual(source_fingerprint(source), before)
            with self.assertRaises(AnalysisError):
                capture_source(source, root / 'invalid', 'HEAD')
            self.assertEqual(source_fingerprint(source), before)


if __name__ == '__main__':
    unittest.main()
