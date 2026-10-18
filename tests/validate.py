import hashlib
import json
import pathlib
import subprocess
import argparse
from repo_guard import repository_root, check_tree, fresh_directory, write_new, guarded_path

ROOT = repository_root(__file__)
check_tree(ROOT, ('docs', 'tests', 'target-local'))
parser = argparse.ArgumentParser()
parser.add_argument('--output', required=True, help='Fresh repository-contained evidence directory')
args = parser.parse_args()
BINARY = guarded_path(ROOT, 'target-local/debug/ulvo')
OUT = fresh_directory(ROOT, args.output)

def hashes():
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in (ROOT / 'tests/fixtures').rglob('*') if p.is_file()}

before = hashes()
results = []
for name, args, expected in [
    ('help', ['--help'], 0), ('version', ['--version'], 0),
    ('inventory', ['inventory', '--all-backends', '--json'], 0),
    ('inspect-human', ['inspect', str(ROOT / 'tests/fixtures/valid')], 0),
    ('inspect', ['inspect', str(ROOT / 'tests/fixtures/valid'), '--json'], 0),
    ('invalid', ['inspect', str(ROOT / 'tests/fixtures/invalid'), '--json'], 1),
    ('missing', ['inspect', str(ROOT / 'tests/fixtures/missing'), '--json'], 1),
    ('mutating-rejected', ['install', 'anything'], 2),
]:
    proc = subprocess.run([str(BINARY), *args], cwd=ROOT, capture_output=True,
                          text=True, timeout=120)
    write_new(ROOT, OUT / f'{name}.stdout.txt', proc.stdout)
    write_new(ROOT, OUT / f'{name}.stderr.txt', proc.stderr)
    assert proc.returncode == expected, (name, proc.returncode, proc.stderr)
    if '--json' in args:
        data = json.loads(proc.stdout)
        assert data['schema_version'] == 1
        if name.startswith('inventory'):
            assert data['host_backend'] == 'wsl'
            assert all(t['backend'] in ('windows', 'wsl', 'msys2') for t in data['toolchains'])
            assert not any(t['verification'] == 'verified' for t in data['toolchains'])
            results.append({'inventory': name, 'records': len(data['toolchains']),
                            'backends': sorted(set(t['backend'] for t in data['toolchains']))})
    results.append({'command': [str(BINARY), *args], 'exit_code': proc.returncode})
after = hashes()
assert before == after, 'fixture content changed'
report = {'results': results, 'fixture_hashes_before': before,
          'fixture_hashes_after': after, 'fixtures_unchanged': before == after}
write_new(ROOT, OUT / 'functional.json', json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
