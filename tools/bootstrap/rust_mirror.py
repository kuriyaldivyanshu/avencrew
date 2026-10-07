#!/usr/bin/env python3
"""Prepare a local rustup mirror from an already verified channel manifest."""
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from urllib.parse import urlsplit


def prepare(manifest_path: Path, mirror: Path, host: str, components: list[str]) -> None:
    manifest_bytes = manifest_path.read_bytes()
    manifest = tomllib.loads(manifest_bytes.decode('utf-8'))
    packages = manifest['pkg']
    # Profiles span all platforms (e.g. minimal includes Windows-only rust-mingw).
    # Match rustup's filtering against this host's component list.
    host_components = {
        entry['pkg'] for entry in packages['rust']['target'][host]['components']
        if entry['target'] in (host, '*')
    }
    minimal = [name for name in manifest['profiles']['minimal'] if name in host_components]
    selected = dict.fromkeys([*minimal, *components])
    for name in selected:
        name = manifest.get('renames', {}).get(name, {}).get('to', name)
        targets = packages[name]['target']
        target = targets[host] if host in targets else targets['*']
        if not target['available']:
            raise ValueError(f'Rust component {name} is unavailable for {host}')
        # Mirror all published compression formats so the existing rustup binary
        # can choose its supported format without changing the pinned manifest.
        formats = [prefix for prefix in ('', 'xz_', 'zst_') if prefix + 'url' in target]
        if not formats:
            raise ValueError(f'Rust component {name} has no archives for {host}')
        for prefix in formats:
            url, expected = target[prefix + 'url'], target[prefix + 'hash']
            parsed = urlsplit(url)
            path = Path(parsed.path.lstrip('/'))
            if (parsed.scheme != 'https' or parsed.netloc != 'static.rust-lang.org'
                    or parsed.query or parsed.fragment or '%' in parsed.path
                    or not parsed.path.startswith('/dist/') or '..' in path.parts
                    or not re.fullmatch(r'[0-9a-f]{64}', expected)):
                raise ValueError(f'Invalid Rust archive URL or hash for {name}')
            archive = mirror / path
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.is_file():
                partial = archive.with_name(archive.name + '.partial')
                try:
                    subprocess.run(['curl', '-fsSL', '--retry', '3', '--max-time', '900',
                                    '-o', str(partial), url], check=True)
                    verify(partial, expected)
                    partial.replace(archive)
                finally:
                    partial.unlink(missing_ok=True)
            else:
                verify(archive, expected)
            print(f'OK   {archive.name} {expected}')
    # Publish only after every selected archive has passed verification.
    local_manifest = mirror / 'dist' / manifest_path.name
    local_manifest.parent.mkdir(parents=True, exist_ok=True)
    local_manifest.write_bytes(manifest_bytes)
    local_manifest.with_suffix('.toml.sha256').write_text(
        f'{hashlib.sha256(manifest_bytes).hexdigest()}  {manifest_path.name}\n'
    )


def verify(path: Path, expected: str) -> None:
    with path.open('rb') as archive:
        actual = hashlib.file_digest(archive, 'sha256').hexdigest()
    if actual != expected:
        raise ValueError(f'integrity mismatch for {path.name}: expected={expected} actual={actual}')


if __name__ == '__main__':
    try:
        prepare(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4:])
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f'bootstrap: {error}')
