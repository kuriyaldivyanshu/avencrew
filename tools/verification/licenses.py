#!/usr/bin/env python3
"""Inventory the installed, locked build closure and retain upstream license text.

Stdlib only. Run through dev-env.sh after dependency installation. Outputs are
ignored build artifacts, not a service or a replacement dependency resolver.
"""
import sys

if sys.version_info < (3, 12):
    print("licenses: Python 3.12+ is required; select it as python3 on PATH", file=sys.stderr)
    raise SystemExit(2)

import hashlib
import io
import json
import os
import re
import subprocess
import tarfile
import tomllib
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'build' / 'licenses'
NOTICE_ROOT = Path(__file__).resolve().parent / 'notices'
SOURCES = json.loads((NOTICE_ROOT / 'sources.json').read_text())


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def license_path(name):
    parts = Path(name).parts
    return any(part.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE')) for part in parts)


def main():
    host = re.search(r'^host: (.+)$', command('rustc', '-vV'), re.M).group(1)
    metadata = json.loads(command('cargo', 'metadata', '--format-version', '1',
                                  '--all-features', '--locked', '--filter-platform', host))
    active = {node['id'] for node in metadata['resolve']['nodes']}
    locked = {(p['name'], p['version']): p for p in
              tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']}
    components, inventories, notices, problems, limitations = [], [], [], [], []

    def retained_notice(key, revision=None):
        source = SOURCES.get(key)
        if source is None:
            return []
        if revision is not None and source['revision'] != revision:
            raise RuntimeError(f'upstream revision mismatch: {key}')
        data = (NOTICE_ROOT / source['file']).read_bytes()
        if digest(data) != source['sha256']:
            raise RuntimeError(f'retained upstream notice checksum mismatch: {key}')
        if source.get('notice_missing'):
            limitations.append({'package': key, 'reason': source['limitation'], 'source': source['url']})
        return [(source['url'], data)]

    def add(ecosystem, name, version, license_value, texts, checksum=None):
        if not isinstance(license_value, str) or not license_value:
            problems.append(f'{ecosystem}:{name}@{version}: missing declared license')
            return
        if not texts:
            problems.append(f'{ecosystem}:{name}@{version}: missing upstream license/notice text')
        purl = f'pkg:{ecosystem}/{quote(name, safe="/")}@{version}'
        component = {'type': 'library', 'name': name, 'version': version,
                     'purl': purl, 'licenses': [{'expression': license_value}]}
        if checksum:
            component['hashes'] = [{'alg': 'SHA-256', 'content': checksum}]
        components.append(component)
        inventories.append({'purl': purl, 'declared_license': license_value,
                            'license_files': [{'path': path, 'sha256': digest(data)} for path, data in texts]})
        notices.append(f'\n===== {purl} ({license_value}) =====\n')
        for path, data in texts:
            notices.append(f'\n--- {path} ---\n{data.decode("utf-8", errors="replace")}\n')

    for package in metadata['packages']:
        if package['id'] not in active or package['source'] is None:
            continue  # First-party workspace code is proprietary, not third-party.
        name, version = package['name'], package['version']
        expected = locked[(name, version)]['checksum']
        archives = list((Path(os.environ['CARGO_HOME']) / 'registry/cache').glob(f'*/{name}-{version}.crate'))
        if len(archives) != 1:
            raise RuntimeError(f'expected one cached archive for {name}@{version}')
        data = archives[0].read_bytes()
        if digest(data) != expected:
            raise RuntimeError(f'Cargo archive checksum mismatch: {name}@{version}')
        texts = []
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
            for member in archive.getmembers():
                path = member.name.partition('/')[2]
                if member.isfile() and license_path(path):
                    texts.append((path, archive.extractfile(member).read()))
        if not texts:
            with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
                vcs = json.load(archive.extractfile(f'{name}-{version}/.cargo_vcs_info.json'))
            texts = retained_notice(f'cargo:{name}@{version}', vcs['git']['sha1'])
        add('cargo', name, version, package['license'], sorted(texts), expected)

    # pnpm lists optional packages for other hosts too; only installed paths
    # enter this host's consumed inventory. The lock hash covers the full graph.
    trees = json.loads(command('pnpm', 'list', '--depth', 'Infinity', '--json', '--recursive'))
    seen = set()

    def walk(node):
        path = node.get('path')
        if path and path not in seen:
            seen.add(path)
            folder = Path(path)
            manifest = folder / 'package.json'
            if manifest.is_file() and 'node_modules' in folder.parts:
                package = json.loads(manifest.read_text())
                license_value = package.get('license')
                if isinstance(license_value, dict):
                    license_value = license_value.get('type')
                texts = [(p.name, p.read_bytes()) for p in sorted(folder.iterdir())
                         if p.is_file() and license_path(p.name)]
                for subdir in ['LICENSES', 'licenses']:
                    if (folder / subdir).is_dir():
                        texts.extend((str(p.relative_to(folder)), p.read_bytes())
                                     for p in sorted((folder / subdir).rglob('*')) if p.is_file())
                if not texts and package['name'].startswith(('@biomejs/cli-', '@esbuild/')):
                    wrapper_name = '@biomejs/biome' if package['name'].startswith('@biomejs/cli-') else 'esbuild'
                    wrappers = list((ROOT / 'node_modules/.pnpm').glob(f"{wrapper_name.replace('/', '+')}@{package['version']}/node_modules/{wrapper_name}"))
                    if len(wrappers) != 1:
                        raise RuntimeError(f'missing matching wrapper for {package["name"]}')
                    wrapper = wrappers[0]
                    manifest_license = json.loads((wrapper / 'package.json').read_text())['license']
                    if manifest_license != license_value:
                        raise RuntimeError(f'native/wrapper license mismatch: {package["name"]}')
                    texts = [(f'{wrapper_name}@{package["version"]}/{p.name}', p.read_bytes())
                             for p in sorted(wrapper.iterdir()) if p.is_file() and license_path(p.name)]
                if not texts:
                    texts = retained_notice(f'npm:{package["name"]}@{package["version"]}')
                add('npm', package['name'], package['version'], license_value, texts)
        for section in ['dependencies', 'devDependencies', 'optionalDependencies']:
            for child in node.get(section, {}).values():
                walk(child)

    for tree in trees:
        walk(tree)

    native = ROOT / 'build/native/sqlite' / host / 'LICENSE.sqlite'
    if not native.is_file():
        problems.append('native SQLite distribution notice missing')
    else:
        notices.append('\n===== SQLite public-domain notice =====\n' + native.read_text())
    if host == 'aarch64-apple-darwin':
        distribution = ROOT / 'apps/desktop/node_modules/electron/dist'
        for name in ['LICENSE', 'LICENSES.chromium.html']:
            path = distribution / name
            if not path.is_file():
                problems.append(f'Electron distribution notice missing: {name}')
            else:
                notices.append(f'\n===== Electron/{name} =====\n' + path.read_text())

    if problems:
        raise RuntimeError('\n'.join(problems))
    OUT.mkdir(parents=True, exist_ok=True)
    components.sort(key=lambda entry: entry['purl'])
    inventories.sort(key=lambda entry: entry['purl'])
    bom = {'bomFormat': 'CycloneDX', 'specVersion': '1.6', 'version': 1,
           'components': components}
    (OUT / f'{host}.cdx.json').write_text(json.dumps(bom, indent=2) + '\n')
    inventory = {'target': host, 'scope': 'Installed locked all-feature build/dev/runtime closure; not a released product',
                 'cargo_lock_sha256': digest((ROOT / 'Cargo.lock').read_bytes()),
                 'pnpm_lock_sha256': digest((ROOT / 'pnpm-lock.yaml').read_bytes()),
                 'packages': inventories, 'redistribution_limitations': limitations}
    (OUT / f'{host}.inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    (OUT / f'{host}.NOTICES.txt').write_text(''.join(notices))
    print(f'License inventory: {len(components)} third-party components; target {host}')
    for limitation in limitations:
        print(f'REDISTRIBUTION BLOCKER: {limitation["package"]}: {limitation["reason"]}')
    print(f'CycloneDX, inventory and distribution notices: build/licenses/{host}.*')


if __name__ == '__main__':
    main()
