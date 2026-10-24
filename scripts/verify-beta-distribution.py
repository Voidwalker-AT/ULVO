"""Independently extract and smoke-test a beta ZIP, never installing into the live vault.

Requires psutil from an existing qualified environment. Outputs private receipts;
the public summary must omit local paths and stdout inventories.
"""
import argparse
import hashlib
import io
import json
import os
import posixpath
import re
import subprocess
import tarfile
import time
import zipfile
from pathlib import Path, PurePosixPath

import psutil


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--launch-gui', action='store_true')
    parser.add_argument('--project', type=Path)
    parser.add_argument('--existing-vault', type=Path)
    args = parser.parse_args()
    if args.out.exists():
        raise RuntimeError('Refuse to overwrite verification evidence')
    args.out.mkdir(parents=True)
    checks, commands, findings = {}, [], []
    output = args.out / 'verification-private.json'

    def check(name, value):
        checks[name] = bool(value)
        if not value:
            raise RuntimeError('Failed: ' + name)

    patterns = {
        'private-key': rb'-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----',
        'provider-token': rb'\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,}|sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{24,}|hf_[A-Za-z0-9]{25,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{15,})\b',
        'credential-url': rb'https?://[^\s/:<>"\x00]{1,100}:[^\s/@<>"\x00]{3,100}@',
        'jwt': rb'\beyJ[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{10,}\b',
        'assigned-secret': rb'''(?i)(?:api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret|password)\s*["']?\s*[:=]\s*["']([A-Za-z0-9_./+=-]{16,})["']''',
    }

    def scan(name, data):
        for category, pattern in patterns.items():
            for match in re.finditer(pattern, data):
                findings.append(dict(file=name, category=category, value_sha256=sha(match.group())))

    def safe(name):
        return (not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts
                and ':' not in name and '\\' not in name and not name.startswith('/'))

    try:
        check('immutable-reference', sha(args.reference.read_bytes()) == 'a02dc550c88ed47fc2f847a4943580a15540af703ffe2255a3de1b68e2e9c995')
        stage = args.out / 'extracted portable Unicode \u03a9 space'
        with zipfile.ZipFile(args.archive) as z, zipfile.ZipFile(args.reference) as ref:
            check('zip-crc', z.testzip() is None)
            check('no-duplicate-paths', len(z.namelist()) == len(set(n.casefold() for n in z.namelist())))
            check('safe-paths-no-links', all(safe(i.filename) and (i.external_attr >> 16 & 0o170000) != 0o120000 for i in z.infolist()))
            files = {n: z.read(n) for n in z.namelist()}
            manifest = json.loads(files['build-manifest.json'])
            inv = json.loads(files['legal/DEPENDENCIES.json'])
            expected = {'README.md', 'LICENSE', 'SOURCE-PROVENANCE.json', 'build-manifest.json', 'FILES.sha256',
                        'ULVO Control Center.bat', 'install.ps1', 'uninstall.ps1',
                        'scripts/install-ulvo.ps1', 'scripts/uninstall-ulvo.ps1',
                        'legal/DEPENDENCIES.json', 'legal/MPL-SOURCE-ARCHIVES.zip', 'legal/THIRD-PARTY-NOTICES.txt'}
            expected.update('bin/' + n for n in ['ulvo.exe', 'ulvo-agent.exe', 'ulvo-desktop.exe', 'WebView2Loader.dll'])
            expected.update('docs/' + n for n in ['INSTALLATION.md', 'PRIVACY.md', 'TROUBLESHOOTING.md', 'STAGE4_RELEASE_MATRIX.md',
                                                'BETA_RELEASE_NOTES.md', 'BETA_VERIFICATION.md', 'BETA_REDISTRIBUTION_REVIEW.md'])
            expected.update('legal/' + n['file'] for n in inv['native_notice_files'])
            check('complete-expected-files-no-extras', set(files) == expected)
            check('binary-byte-identity', all(files['bin/' + n] == ref.read('bin/' + n) and sha(files['bin/' + n]) == h for n, h in manifest['binaries'].items()))
            check('installer-byte-identity', all(files[n] == ref.read(n) for n in ['install.ps1', 'uninstall.ps1', 'scripts/install-ulvo.ps1', 'scripts/uninstall-ulvo.ps1']))
            check('manifest-content-hashes', all(sha(files[e['path']]) == e['sha256'] and len(files[e['path']]) == e['bytes'] for e in manifest['content_files'])
                  and {e['path'] for e in manifest['content_files']} == set(files) - {'build-manifest.json', 'FILES.sha256'})
            check('per-file-checksums', {n: h for h, n in (line.split('  ', 1) for line in files['FILES.sha256'].decode().splitlines())}
                  == {n: sha(b) for n, b in files.items() if n != 'FILES.sha256'})
            check('mit-license', b'Copyright (c) 2026 Voidwalker-AT' in files['LICENSE'] and b'Permission is hereby granted, free of charge' in files['LICENSE'])
            check('native-notice-hashes', all(sha(files['legal/' + n['file']]) == n['sha256'] for n in inv['native_notice_files']))
            check('runtime-license-texts-present', all(p['notice_texts'] for p in inv['components'] if 'runtime' in p['roles']))
            broken = []
            for n, data in files.items():
                scan(n, data)
                if n.lower().endswith(('.exe', '.dll')):
                    scan(n + ':utf16', data[:len(data) // 2 * 2].decode('utf-16-le', errors='ignore').encode('utf8'))
                else:
                    check('no-private-host-paths:' + n, b'E:\\Workspace' not in data and b'C:\\Users\\hp' not in data)
                if not n.endswith('.md'):
                    continue
                for link in re.findall(r'\]\(([^)]+)\)', data.decode('utf8')):
                    if '://' in link or link.startswith('#'):
                        continue
                    target = posixpath.normpath(posixpath.join(posixpath.dirname(n), link.split('#')[0]))
                    if target not in files:
                        broken.append(dict(document=n, target=target))
            check('relative-document-links', not broken)
            readme = files['README.md'].decode('utf8')
            check('accurate-beta-readme', all(s in readme for s in ['v1.2.0-beta.1', 'Windows 11 x64 Early Access', 'WebView2', 'experimental', 'seven crates', '1.2.0']))
            check('no-stale-readme-claims', '**No GUI**' not in readme and 'Windows 11 / Windows 10' not in readme and 'ProofStack' not in readme)
            with zipfile.ZipFile(io.BytesIO(files['legal/MPL-SOURCE-ARCHIVES.zip'])) as sources:
                check('mpl-source-zip-crc-paths', sources.testzip() is None and all(safe(n) for n in sources.namelist()))
                crates = {n: sources.read(n) for n in sources.namelist() if n.endswith('.crate')}
                mpl = [p for p in inv['components'] if 'MPL' in (p.get('declared_license') or '') and p['component'].startswith('Cargo/')]
                check('mpl-original-source-checksums', len(crates) == len(mpl) == 5 and all(sha(crates[p['component'].split('/')[1] + '-' + p['version'] + '.crate']) == p['lock_checksum'] for p in mpl))
                for n, b in crates.items():
                    with tarfile.open(fileobj=io.BytesIO(b)) as tar:
                        check('safe-mpl-tar:' + n, all(safe(e.name) and not e.issym() and not e.islnk() for e in tar))
                        for e in tar:
                            if e.isfile():
                                scan('mpl-source/' + e.name, tar.extractfile(e).read())
            check('no-secret-pattern-candidates', not findings)
            z.extractall(stage)
            check('extracted-every-file-hash', all(sha((stage / n).read_bytes()) == sha(b) for n, b in files.items()))
        cli = stage / 'bin/ulvo.exe'
        config = args.out / 'isolated config \u03a9 space'
        vault = args.out / 'isolated vault \u03a9 space'
        env = os.environ.copy()
        env.pop('ULVO_CONFIG_ROOT', None)
        env.pop('ULVO_VAULT_ROOT', None)
        env['PATH'] = os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32')

        def run(*words, test_vault=None, timeout=30):
            prefix = [str(cli), '--config-root', str(config)]
            if test_vault:
                prefix += ['--vault-root', str(test_vault)]
            start = time.monotonic()
            p = subprocess.run(prefix + list(map(str, words)), cwd=stage, env=env, stdin=subprocess.DEVNULL,
                               capture_output=True, text=True, encoding='utf8', timeout=timeout)
            commands.append(dict(args=list(map(str, words)), exit=p.returncode, ms=round((time.monotonic() - start) * 1000), stdout=p.stdout, stderr=p.stderr))
            check('command-' + str(len(commands)), p.returncode == 0)
            return p.stdout

        run('--version')
        run('--help')
        run('setup', 'status')
        plan = json.loads(run('setup', 'plan', '--vault', vault))
        run('setup', 'apply', plan['plan']['plan_id'])
        run('doctor', '--deep', '--json')
        sentinel = vault / 'preserve-user-owned-sentinel.txt'
        sentinel.write_bytes(b'preserve user data; no uninstall or reclamation authorized\n')
        before_sentinel = sha(sentinel.read_bytes())
        started = False
        try:
            stdout = run('agent', 'start', timeout=12)
            started = True
            pid = int(re.search(r'PID: (\d+)', stdout)[1])
            agent = psutil.Process(pid)
            check('isolated-agent-identity', Path(agent.exe()).resolve() == (stage / 'bin/ulvo-agent.exe').resolve())
            check('cli-output-closed-with-agent-alive', agent.is_running())
            run('agent', 'status')
        finally:
            if started:
                run('agent', 'stop', timeout=12)
        for _ in range(20):
            if not psutil.pid_exists(pid):
                break
            time.sleep(0.1)
        check('isolated-agent-stopped', not psutil.pid_exists(pid))
        check('vault-sentinel-preserved', sentinel.is_file() and sha(sentinel.read_bytes()) == before_sentinel)
        check('no-development-server-listener', not any(c.status == psutil.CONN_LISTEN and c.laddr.port == 1420 for c in psutil.net_connections(kind='tcp')))
        if args.project:
            if not args.existing_vault:
                raise RuntimeError('--existing-vault required for existing project')
            # No tool installs: use the existing connector/environment and its pinned locks.
            run('run', args.project, '--', 'python', '-m', 'unittest',
                'tests.test_config_portability', 'tests.test_scorer_verification', '-v',
                test_vault=args.existing_vault, timeout=120)
            check('project-19-tests-passed', 'Ran 19 tests' in commands[-1]['stderr'] and 'OK' in commands[-1]['stderr'])
        if args.launch_gui:
            gui_env = env.copy()
            gui_env['ULVO_CONFIG_ROOT'] = str(config)
            gui_env['ULVO_VAULT_ROOT'] = str(vault)
            gui = subprocess.Popen([str(stage / 'bin/ulvo-desktop.exe')], env=gui_env, cwd=stage,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(3)
            check('gui-process-started', gui.poll() is None)
            (args.out / 'gui-process-private.json').write_text(json.dumps(dict(pid=gui.pid, exe=str(stage / 'bin/ulvo-desktop.exe')), indent=2), encoding='utf8')
            checks['gui-native-window-inspection'] = 'PENDING'
            print('GUI ready for native inspection; keeping the gateway job alive.', flush=True)
            # The ULVO gateway supervises child lifetimes. Keep it alive until the
            # operator records a real native-window inspection and closes our GUI.
            inspection = args.out / 'gui-inspection.json'
            deadline = time.monotonic() + 240
            while time.monotonic() < deadline and not inspection.exists() and gui.poll() is None:
                time.sleep(0.5)
            check('gui-inspection-receipt-present', inspection.exists())
            inspected = json.loads(inspection.read_text(encoding='utf8'))
            check('gui-native-window-inspection', inspected.get('passed') is True and inspected.get('pid') == gui.pid)
            gui.wait(timeout=30)
            check('gui-closed-cleanly', gui.returncode == 0)
        result = dict(passed=all(v is True for k, v in checks.items() if k != 'gui-native-window-inspection'),
                      archive_sha256=sha(args.archive.read_bytes()), checks=checks, commands=commands,
                      findings=findings, gui_inspection='PASSED' if args.launch_gui else 'NOT RUN',
                      scope='Same-host corrected ZIP smoke tests; not independent clean Windows or installer acceptance')
        output.write_text(json.dumps(result, indent=2), encoding='utf8')
        print(json.dumps(dict(passed=result['passed'], checks=len(checks), commands=len(commands), archive_sha256=result['archive_sha256'], gui_inspection=result['gui_inspection']), indent=2))
    except Exception as error:
        output.write_text(json.dumps(dict(passed=False, error=str(error), checks=checks, commands=commands, findings=findings), indent=2), encoding='utf8')
        raise


if __name__ == '__main__':
    main()
