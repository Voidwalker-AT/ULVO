import os
import pathlib
import stat
import tomllib


def reject_links(path):
    path = pathlib.Path(os.path.abspath(path))
    for node in (path, *path.parents):
        try:
            meta = node.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(meta.st_mode) or getattr(meta, 'st_file_attributes', 0) & 0x400:
            raise RuntimeError(f'link/reparse traversal rejected: {node}')
    return path


def repository_root(script):
    original = reject_links(script)
    if not original.is_file():
        raise RuntimeError('repository helper script is missing')
    root = original.parents[1]
    for name in ('Cargo.toml', 'Cargo.lock', '.git'):
        reject_links(root / name)
        if not (root / name).exists():
            raise RuntimeError('repository sentinels missing')
    manifest = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))
    if set(manifest.get('workspace', {}).get('members', [])) != {
        f'crates/ulvo-{part}' for part in ('core', 'adapters', 'discovery', 'cli')
    }:
        raise RuntimeError('not the expected ULVO workspace')
    return root


def guarded_path(root, path):
    root = reject_links(root)
    path = pathlib.Path(path)
    if not path.is_absolute():
        path = root / path
    if '..' in path.parts:
        raise RuntimeError('parent traversal rejected')
    path = reject_links(path)
    if not path.is_relative_to(root):
        raise RuntimeError('output outside repository')
    return path


def check_tree(root, bases):
    for base in bases:
        path = guarded_path(root, base)
        for node in path.rglob('*'):
            guarded_path(root, node)


def fresh_directory(root, path):
    path = guarded_path(root, path)
    if not path.parent.is_dir():
        raise RuntimeError('output parent must exist')
    path.mkdir(exist_ok=False)
    return path


def write_new(root, path, text):
    path = guarded_path(root, path)
    with path.open('x', encoding='utf-8') as stream:
        stream.write(text)
