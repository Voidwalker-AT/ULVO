"""Assemble a corrected beta from an immutable reference, preserving binary bytes.

No builds, deployment, downloads or toolchain changes. Refuses existing output.
Run with --help; --allow-dirty is only for explicitly labeled private preflight.
"""
import argparse
import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

REFERENCE_SHA = 'a02dc550c88ed47fc2f847a4943580a15540af703ffe2255a3de1b68e2e9c995'
BINARY_SOURCE = 'd112aac3c13c54022ba54e78488de49e019147ff'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--git', default='git')
    parser.add_argument('--allow-dirty', action='store_true')
    parser.add_argument('--inherited-evidence', type=Path)
    parser.add_argument('--desktop', type=Path, help='Explicitly rebuilt desktop; CLI, agent and loader remain reference bytes')
    parser.add_argument('--desktop-source-commit', help='Exact source revision used for the rebuilt desktop')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]

    def git(*words):
        return subprocess.check_output([args.git, '-C', str(root), *words], text=True, encoding='utf8').strip()

    dirty = bool(git('status', '--porcelain'))
    if dirty and not args.allow_dirty:
        raise RuntimeError('Commit verified source changes before final packaging')
    if args.out.exists():
        raise RuntimeError('Refuse to overwrite previous candidate/evidence')
    if sha(args.reference.read_bytes()) != REFERENCE_SHA:
        raise RuntimeError('Immutable reference SHA-256 mismatch')
    git('cat-file', '-e', BINARY_SOURCE + '^{commit}')
    files = {}
    with zipfile.ZipFile(args.reference) as z:
        if z.testzip() is not None:
            raise RuntimeError('Reference CRC failure')
        manifest = json.loads(z.read('build-manifest.json'))
        if manifest['source_commit'] != BINARY_SOURCE:
            raise RuntimeError('Reference source mismatch')
        for name, value in manifest['binaries'].items():
            data = z.read('bin/' + name)
            if sha(data) != value:
                raise RuntimeError('Reference binary manifest mismatch')
            files['bin/' + name] = data
        for name in ['install.ps1', 'uninstall.ps1', 'scripts/install-ulvo.ps1',
                     'scripts/uninstall-ulvo.ps1', 'ULVO Control Center.bat']:
            files[name] = z.read(name)
    docs = ['README.md', 'LICENSE', 'docs/INSTALLATION.md', 'docs/PRIVACY.md',
            'docs/TROUBLESHOOTING.md', 'docs/STAGE4_RELEASE_MATRIX.md',
            'docs/BETA_RELEASE_NOTES.md', 'docs/BETA_REDISTRIBUTION_REVIEW.md',
            'docs/BETA_VERIFICATION.md']
    if args.desktop:
        if not args.desktop.is_file():
            raise RuntimeError('Rebuilt desktop is absent')
        files['bin/ulvo-desktop.exe'] = args.desktop.read_bytes()
    for name in docs:
        # Commit bytes avoid checkout-specific CRLF conversion in public assets.
        files[name] = subprocess.check_output([args.git, '-C', str(root), 'show', 'HEAD:' + name]) if not dirty else (root / name).read_bytes()
    for p in sorted((root / 'legal').rglob('*')):
        if p.is_symlink():
            raise RuntimeError('Refuse linked legal input')
        if p.is_file():
            name = p.relative_to(root).as_posix()
            files[name] = subprocess.check_output([args.git, '-C', str(root), 'show', 'HEAD:' + name]) if not dirty else p.read_bytes()
    provenance = dict(binary_source_commit=BINARY_SOURCE,
                      packaging_source_commit=git('rev-parse', 'HEAD'),
                      packaging_source_tree=git('rev-parse', 'HEAD^{tree}'),
                      packaging_working_tree_dirty=dirty,
                      immutable_reference_archive_sha256=REFERENCE_SHA,
                      binaries_rebuilt=bool(args.desktop), binaries_stripped=False,
                      binary_sources={n: ((args.desktop_source_commit or git('rev-parse', 'HEAD')) if args.desktop and n == 'ulvo-desktop.exe' else BINARY_SOURCE) for n in manifest['binaries']},
                      cargo_lock_sha256=sha((root / 'Cargo.lock').read_bytes()),
                      npm_lock_sha256=sha((root / 'apps/ulvo-desktop/package-lock.json').read_bytes()))
    if args.inherited_evidence:
        names = ['final-workspace-tests.json', 'agent-final-tests.txt',
                 'archive-acceptance.json', 'crossmodal-r2-regression.txt',
                 'crossmodal-stage4-preflight.json', 'installer-security-recovery-final.json',
                 'installer-final/installer-acceptance-verification.json', 'STAGE4_READINESS_REPORT.md']
        provenance['inherited_evidence_receipts'] = [dict(receipt=n, sha256=sha((args.inherited_evidence / n).read_bytes())) for n in names]
    files['SOURCE-PROVENANCE.json'] = (json.dumps(provenance, indent=2) + '\n').encode()
    manifest = dict(release='v1.2.0-beta.1', channel='GitHub Pre-release',
                    support='Windows 11 x64 Early Access', stable_release=False,
                    executable_version='1.2.0', provenance=provenance,
                    desktop_runtime='Microsoft WebView2 Evergreen (separately installed)',
                    root_installer='experimental; not accepted', legal_clearance='human review pending',
                    binaries={n: sha(files['bin/' + n]) for n in manifest['binaries']},
                    content_files=[dict(path=n, bytes=len(b), sha256=sha(b)) for n, b in sorted(files.items())],
                    hashing_scope='content_files excludes this manifest and FILES.sha256; FILES.sha256 covers manifest and all other content, excluding itself')
    files['build-manifest.json'] = (json.dumps(manifest, indent=2) + '\n').encode()
    files['FILES.sha256'] = ''.join(sha(b) + '  ' + n + '\n' for n, b in sorted(files.items())).encode()
    args.out.mkdir(parents=True)
    archive = args.out / 'ulvo-v1.2.0-beta.1-windows-x64-portable.zip'
    with zipfile.ZipFile(archive, 'x', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in sorted(files.items()):
            entry = zipfile.ZipInfo(name, (2026, 10, 24, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            z.writestr(entry, data)
    for name in ['FILES.sha256', 'build-manifest.json', 'SOURCE-PROVENANCE.json']:
        (args.out / name).write_bytes(files[name])
    (args.out / 'SHA256SUMS.txt').write_text(sha(archive.read_bytes()) + '  ' + archive.name + '\n', encoding='ascii')
    print(json.dumps(dict(archive=str(archive), sha256=sha(archive.read_bytes()), files=len(files), provenance=provenance), indent=2))


if __name__ == '__main__':
    main()
