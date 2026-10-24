"""Collect release notices without installing dependencies or rebuilding binaries.

Input: Cargo metadata generated with --locked --offline --filter-platform
x86_64-pc-windows-gnu --features tauri/custom-protocol, and reviewed supplemental
upstream license files. The inventory deliberately over-approximates linked code.
"""
import argparse
import collections
import hashlib
import json
import re
import tarfile
import tomllib
import zipfile
from pathlib import Path


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--metadata', type=Path, required=True)
    parser.add_argument('--supplement-dir', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    meta = json.loads(args.metadata.read_text(encoding='utf8'))
    lock = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf8'))
    packages = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    roles = collections.defaultdict(set)

    def visit(identifier, role):
        if role in roles[identifier]:
            return
        roles[identifier].add(role)
        for dep in nodes[identifier]['deps']:
            for kind in dep['dep_kinds']:
                if kind['kind'] == 'dev':
                    continue
                proc = any('proc-macro' in t['kind'] for t in packages[dep['pkg']]['targets'])
                next_role = 'build' if kind['kind'] == 'build' or proc or role == 'build' else role
                visit(dep['pkg'], next_role)

    for identifier in meta['workspace_members']:
        if packages[identifier]['name'] in ('ulvo-cli', 'ulvo-agent', 'ulvo-desktop'):
            visit(identifier, 'runtime')
    for identifier in meta['workspace_members']:
        for dep in nodes[identifier]['deps']:
            if any(k['kind'] == 'dev' for k in dep['dep_kinds']):
                visit(dep['pkg'], 'development')

    out = args.out
    out.mkdir(parents=True, exist_ok=False)
    (out / 'native').mkdir()
    records, texts, source_archives = [], [], []
    provenance = json.loads((args.supplement_dir / 'upstream-provenance.json').read_text(encoding='utf8'))
    provenance = [p for p in provenance if p['status'] == 'FETCHED']
    supplements = {p['file']: p for p in provenance}
    missing = []

    def collect(component, version, license_id, base, source, component_roles, checksum=None, explicit=None):
        files = []
        if base.is_dir():
            for p in sorted(base.rglob('*')):
                if p.is_file() and not p.is_symlink() and p.stat().st_size < 1_048_576:
                    if re.match(r'(?i)^(?:licen[cs]e|copying|copyright|notice)(?:$|[-_.])', p.name):
                        files.append((p.relative_to(base).as_posix(), p.read_bytes(), source))
            if explicit and (base / explicit).is_file() and not any(n == explicit for n, _, _ in files):
                files.append((explicit, (base / explicit).read_bytes(), source))
        short = component.split('/', 1)[1]
        if not files and component.startswith('Cargo/'):
            key = short + '-LICENSE-MIT' if short == 'tauri-plugin' else 'MPL-2.0.txt' if short == 'selectors' else short + '-LICENSE'
            if key in supplements:
                files.append((key, (args.supplement_dir / 'upstream' / key).read_bytes(), supplements[key]['url']))
        record = dict(component=component, version=version, declared_license=license_id,
                      roles=sorted(component_roles), source=source, lock_checksum=checksum,
                      notice_texts=[dict(file=n, sha256=digest(b), source=u) for n, b, u in files])
        records.append(record)
        if not files:
            missing.append(dict(component=component, version=version, roles=sorted(component_roles)))
        if 'runtime' in component_roles and not files:
            raise RuntimeError('Missing potentially shipped notice: ' + component)
        if files:
            texts.append(f'\n\n---\n{component} {version}\nDeclared license: {license_id}\nRoles: {", ".join(sorted(component_roles))}\nSource: {source}\n')
            for name, data, url in files:
                texts.append(f'\n[{name}]\nRetrieved from: {url}\nSHA-256: {digest(data)}\n' + data.decode('utf8', errors='replace'))
        if component.startswith('Cargo/') and license_id and 'MPL' in license_id:
            candidates = list((root / 'vault/runtimes/rust/cargo/registry/cache').glob(f'*/{short}-{version}.crate'))
            if len(candidates) != 1 or digest(candidates[0].read_bytes()) != checksum:
                raise RuntimeError('MPL source archive absent or checksum mismatch: ' + component)
            with tarfile.open(candidates[0]) as archive:
                for entry in archive:
                    if entry.name.startswith('/') or '..' in Path(entry.name).parts or entry.issym() or entry.islnk():
                        raise RuntimeError('Unsafe third-party source archive')
            source_archives.append((candidates[0], record))

    known = {(p['name'], p['version']): p for p in meta['packages']}
    for p in lock['package']:
        if 'source' not in p:
            continue  # first-party workspace packages
        q = known.get((p['name'], p['version']))
        if q:
            collect('Cargo/' + p['name'], p['version'], q.get('license'), Path(q['manifest_path']).parent,
                    f'https://static.crates.io/crates/{p["name"]}/{p["name"]}-{p["version"]}.crate',
                    roles[q['id']], p.get('checksum'), q.get('license_file'))
        else:
            records.append(dict(component='Cargo/' + p['name'], version=p['version'],
                                roles=['not-selected-for-windows-target'], declared_license=None,
                                source=f'https://crates.io/crates/{p["name"]}/{p["version"]}',
                                lock_checksum=p.get('checksum'), notice_texts=[],
                                review='Not in target-filtered resolve graph; not distributed in this portable package. License not reviewed.'))
    npm = json.loads((root / 'apps/ulvo-desktop/package-lock.json').read_text(encoding='utf8'))
    for name, p in npm['packages'].items():
        if not name:
            continue
        component_roles = ['build'] if p.get('dev') else ['runtime']
        if p.get('optional') and not (root / 'apps/ulvo-desktop' / name).is_dir():
            component_roles = ['non-host-optional-build']
        collect('npm/' + name.removeprefix('node_modules/'), p['version'], p.get('license'),
                root / 'apps/ulvo-desktop' / name, p.get('resolved'), component_roles, p.get('integrity'))

    header = '''ULVO THIRD-PARTY NOTICES

This file preserves collected license and attribution texts. The inventory
distinguishes conservative runtime closure, build tools, development dependencies
and non-target lock entries. Runtime classification is not a linked-binary SBOM;
Cargo metadata unifies some features across build and target contexts.
All third-party terms remain applicable. See docs/BETA_REDISTRIBUTION_REVIEW.md.
Native runtime/loader texts are also in legal/native. MPL source is supplied in
legal/MPL-SOURCE-ARCHIVES.zip; these are original checksum-verified registry
source archives, with no ULVO changes. For each such component, extract its .crate
with a tar-compatible tool. Its registry URL and checksum are in DEPENDENCIES.json.
'''
    (out / 'THIRD-PARTY-NOTICES.txt').write_text(header + ''.join(texts), encoding='utf8')
    with zipfile.ZipFile(out / 'MPL-SOURCE-ARCHIVES.zip', 'x', zipfile.ZIP_DEFLATED) as z:
        for path, record in source_archives:
            z.write(path, path.name)
        z.writestr('SOURCE-INFORMATION.txt', 'Original source archives, unmodified by ULVO.\n' + '\n'.join(
            f'{p.name}  SHA-256 {record["lock_checksum"]}  {record["source"]}' for p, record in source_archives))
    native = []

    def copy_native(path, name, source):
        data = path.read_bytes()
        (out / 'native' / name).write_bytes(data)
        native.append(dict(file='native/' + name, sha256=digest(data), source=source))

    rust = root / 'vault/runtimes/rust/rustup/toolchains/1.96.0-x86_64-pc-windows-gnu/share/doc/rust'
    copy_native(rust / 'COPYRIGHT-library.html', 'RUST-1.96.0-COPYRIGHT-library.html',
                'Official locally installed rust-std 1.96.0 distribution; https://github.com/rust-lang/rust/blob/1.96.0/COPYRIGHT')
    for p in (args.supplement_dir / 'upstream').glob('WebView2-*'):
        copy_native(p, p.name, 'Microsoft.Web.WebView2 1.0.3800.47 NuGet package')
    for p in (args.supplement_dir / 'upstream').glob('Rust-*'):
        copy_native(p, p.name, supplements[p.name]['url'])
    for name in ['GCC-COPYING.RUNTIME', 'GCC-COPYING3', 'MPL-2.0.txt']:
        copy_native(args.supplement_dir / 'upstream' / name, name, supplements[name]['url'])
    for package in ['gcc-libs', 'crt', 'headers', 'libwinpthread']:
        for p in sorted((Path('C:/msys64/ucrt64/share/licenses') / package).glob('*')):
            if p.is_file():
                copy_native(p, 'MSYS2-' + package + '-' + p.name, 'Local MSYS2 UCRT64 license materials; exact linked membership not established')
    webview = json.loads((args.supplement_dir / 'webview-provenance.json').read_text(encoding='utf8'))
    inventory = dict(schema_version=1, target='x86_64-pc-windows-gnu',
                     scope='Complete Cargo.lock and npm package-lock enumeration; target metadata closure is conservative, not exact binary membership',
                     cargo_metadata_packages=len(packages), cargo_lock_packages=len(lock['package']),
                     counts=dict(collections.Counter('+'.join(p['roles']) for p in records)),
                     missing_texts_for_nonshipped_tools=missing, components=records,
                     cargo_lock_graph=[dict(name=p['name'], version=p['version'], dependencies=p.get('dependencies', [])) for p in lock['package']],
                     target_graph=[dict(package=packages[n['id']]['name'] + '@' + packages[n['id']]['version'],
                                        features=n['features'], dependencies=[dict(package=packages[d['pkg']]['name'] + '@' + packages[d['pkg']]['version'],
                                                                                kinds=d['dep_kinds']) for d in n['deps']]) for n in meta['resolve']['nodes']],
                     native_notice_files=native, retrieved_upstream=provenance, webview2=webview)
    (out / 'DEPENDENCIES.json').write_text(json.dumps(inventory, indent=2) + '\n', encoding='utf8')
    print(json.dumps(dict(components=len(records), counts=inventory['counts'], missing_nonshipped=len(missing), mpl_source_archives=len(source_archives)), indent=2))


if __name__ == '__main__':
    main()
