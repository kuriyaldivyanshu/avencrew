#!/usr/bin/env python3
"""Archive extraction that refuses absolute paths, traversal and link escapes.

Used by tools/bootstrap/bootstrap.sh. Archives are staged inside the managed
root and validated before anything is written to an activation directory.
"""
import os
import sys
import tarfile

LINK_TARGETS = ('symlink', 'lnk', 'hardlink')


def member_is_safe(name: str) -> bool:
    """Reject absolute paths, drive-ish paths and any '..' traversal."""
    if not name or name.startswith('/') or name.startswith('\\'):
        return False
    if os.path.isabs(name):
        return False
    parts = name.replace('\\', '/').split('/')
    return '..' not in parts


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
        print('usage: safe_extract.py <archive> <format: tgz|tar.xz> <dest>', file=sys.stderr)
        return 2
    archive, fmt, dest = sys.argv[1], sys.argv[2], sys.argv[3]
    mode = 'r:gz' if fmt == 'tgz' else 'r:xz'
    os.makedirs(dest, exist_ok=True)
    with tarfile.open(archive, mode) as tar:
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
        # Strip exactly one leading './' and refuse to escape dest.
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


if __name__ == '__main__':
    sys.exit(main())