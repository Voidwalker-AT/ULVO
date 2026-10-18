import ast
import pathlib
import subprocess
import tomllib
import unittest
import stat
import uuid
from types import SimpleNamespace
from unittest.mock import patch
from repo_guard import repository_root, guarded_path, fresh_directory, write_new

ROOT = repository_root(__file__)


class HardeningTests(unittest.TestCase):
    def test_relocated_root_and_missing_sentinel(self):
        base = fresh_directory(ROOT, f'docs/evidence/hardening-20261016/relocated-{uuid.uuid4().hex}')
        for part in ('tests', '.git'):
            guarded_path(ROOT, base / part).mkdir()
        write_new(ROOT, base / 'Cargo.toml', (ROOT / 'Cargo.toml').read_text())
        write_new(ROOT, base / 'Cargo.lock', '')
        write_new(ROOT, base / 'tests/helper.py', '')
        self.assertEqual(repository_root(base / 'tests/helper.py'), base)
        with self.assertRaises(RuntimeError):
            repository_root(base / 'absent/helper.py')

    def test_escape_rejected(self):
        with self.assertRaises(RuntimeError):
            guarded_path(ROOT, ROOT.parent / 'outside.txt')
        with self.assertRaises(RuntimeError):
            guarded_path(ROOT, 'docs/../outside.txt')

    def test_link_and_collision_rejected(self):
        out = fresh_directory(ROOT, f'docs/evidence/hardening-20261016/guard-case-{uuid.uuid4().hex}')
        write_new(ROOT, out / 'file.txt', 'preserve')
        with self.assertRaises(FileExistsError):
            write_new(ROOT, out / 'file.txt', 'overwrite')
        with self.assertRaises(FileExistsError):
            fresh_directory(ROOT, out)
        link = guarded_path(ROOT, out / 'linked')
        with patch.object(pathlib.Path, 'lstat', return_value=SimpleNamespace(st_mode=stat.S_IFLNK)):
            with self.assertRaises(RuntimeError):
                guarded_path(ROOT, link)
        self.assertEqual((out / 'file.txt').read_text(), 'preserve')

    def test_ignore_rules(self):
        paths = ['target/test', 'crates/ulvo-core/target/test', '__pycache__/x.pyc',
                 'tests/__pycache__/x.pyc', 'tests/x.pyo', '.cargo-local/test', 'target-local/test']
        proc = subprocess.run(['git', '-C', str(ROOT), 'check-ignore', *paths],
                              capture_output=True, text=True, check=True)
        self.assertEqual(set(proc.stdout.splitlines()), set(paths))

    def test_helpers_parse_and_no_automatic_verify(self):
        for name in ('validate.py', 'report.py', 'repo_guard.py'):
            text = (ROOT / 'tests' / name).read_text()
            ast.parse(text)
            self.assertNotIn('/mnt/e/Workspace/Active/ULVO', text)
        self.assertNotIn("'--verify'", (ROOT / 'tests/validate.py').read_text())

    def test_report_dependency_table_matches_lock(self):
        report = (ROOT / 'docs/PHASE0_IMPLEMENTATION_REPORT.md').read_text()
        packages = tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']
        for p in packages:
            self.assertIn(f'| {p["name"]} | {p["version"]} |', report)
        self.assertIn('`docs/evidence/13-git-review.txt`', report)


if __name__ == '__main__':
    unittest.main(verbosity=2)
