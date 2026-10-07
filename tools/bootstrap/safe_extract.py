#!/usr/bin/env python3
"""Archive extraction that refuses absolute paths, traversal and link escapes.

Used by tools/bootstrap/bootstrap.sh and tools/bootstrap/build-sqlite.sh.
Archives are staged inside the managed root and validated before anything is
written to an activation directory.

Supported formats: tgz, tar.xz, zip.
"""
import os
import stat
import sys
import tarfile
import zipfile

LINK_TARGETS = ('symlink', 'lnk', 'hardlink')


def member_is_safe(name: str) -> bool:
    """Reject absolute paths, drive-ish paths and any '..' traversal."""
    if not name or name.startswith('/') or name.startswith('\\'):
        return False
    if os.path.isabs(name):
        return False
    parts = name.replace('\\', '/').split('/')
    return '..' not in parts


def extract_tar(archive: str, dest: str) -> int:
    with tarfile.open(archive, 'r:gz' if archive.endswith('.tgz') else 'r:xz') as tar:
        members = tar.getmembers()
        names = {m.name.lstrip('./') for m in members}
        for member in members:
            if not member_is_safe(member.name):
                print(f'REJECT unsafe member path: {member.name}', file=sys.stderr)
                return 1
            if member.islnk() or member.issym():
                if not link_is_contained(member, names):
                    print(f'REJECT escaping link: {member.name} -> {member.linkname}', file=sys.stderr)
                    return 1
            if not (member.isfile() or member.isdir() or member.islnk() or member.issym()):
                print(f'REJECT unexpected member type: {member.name}', file=sys.stderr)
                return 1
        for member in members:
            rel = member.name.lstrip('./')
            if not rel:
                continue
            target = os.path.normpath(os.path.join(dest, rel))
            if not (target == dest or target.startswith(dest + os.sep)):
                print(f'REJECT escape after normalisation: {member.name}', file=sys.stderr)
                return 1
        tar.extractall(dest)
        print(f'extracted {len(members)} members -> {dest}')
        return 0


def extract_zip(archive: str, dest: str) -> int:
    """Zip has no portable symlink handling here, so refuse encoded symlinks."""
    with zipfile.ZipFile(archive) as zf:
        infos = zf.infolist()
        for info in infos:
            name = info.filename
            if not member_is_safe(name):
                print(f'REJECT unsafe member path: {name}', file=sys.stderr)
                return 1
            mode = info.external_attr >> 16
            if mode and stat.S_ISLNK(mode):
                print(f'REJECT symlink member in zip: {name}', file=sys.stderr)
                return 1
            if mode and not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                print(f'REJECT unexpected member type: {name}', file=sys.stderr)
                return 1
        for info in infos:
            rel = info.filename.lstrip('./')
            if not rel:
                continue
            target = os.path.normpath(os.path.join(dest, rel))
            if not (target == dest or target.startswith(dest + os.sep)):
                print(f'REJECT escape after normalisation: {info.filename}', file=sys.stderr)
                return 1
        zf.extractall(dest)
        print(f'extracted {len(infos)} members -> {dest}')
        return 0


def link_is_contained(member: tarfile.TarInfo, archive_names: set[str]) -> bool:
    """A link must resolve inside the extraction root and name an archive member.

    A relative target containing '..' is legitimate when it stays inside the
    tree (Node ships bin/corepack -> ../lib/node_modules/...). So the target is
    resolved against the link's own directory FIRST, and containment is judged
    on that resolved path. Absolute targets are rejected outright.
    """
    target = member.linkname
    if not target:
        return False
    if target.startswith('/') or target.startswith('\\') or os.path.isabs(target):
        return False
    base = os.path.dirname(member.name)
    resolved = os.path.normpath(os.path.join(base, target))
    # Escapes the archive root, in any depth of '../'.
    if resolved.startswith('..') or os.path.isabs(resolved):
        return False
    return resolved in archive_names


def main() -> int:
    if len(sys.argv) != 4:
        print('usage: safe_extract.py <archive> <format: tgz|tar.xz|zip> <dest>', file=sys.stderr)
        return 2
    archive, fmt, dest = sys.argv[1], sys.argv[2], sys.argv[3]
    if fmt not in ('tgz', 'tar.xz', 'zip'):
        print(f'unknown format {fmt!r}', file=sys.stderr)
        return 2
    os.makedirs(dest, exist_ok=True)
    if fmt == 'zip':
        return extract_zip(archive, dest)
    return extract_tar(archive, dest)


if __name__ == '__main__':
    sys.exit(main())