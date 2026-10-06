#!/usr/bin/env python3
"""Read and validate tools/bootstrap/pins.json.

This is the only place the bootstrap scripts learn versions, URLs and digests.
The file is tracked and self-contained, so a clean checkout never needs any
private design document to build.

Usage:
    pins.py --platform                 exit 0 if this host is supported
    pins.py --tool-version <tool>      print a tool version
    pins.py --field <tool> <field>     print one field
    pins.py --list                     print tool names

Exits 2 with a clear message for an unsupported platform, an unknown tool or a
missing field. Never guesses, never falls back to `latest`.
"""
import json
import platform
import sys
from pathlib import Path

PINS = Path(__file__).resolve().parent / 'pins.json'
SUPPORTED_SCHEMA = '1.0'
SUPPORTED_PLATFORMS = {'darwin-arm64', 'linux-arm64'}

REQUIRED_FIELDS = {
    'rust': ['version', 'host', 'channel_manifest_url', 'channel_manifest_sha256',
             'channel_manifest_bytes', 'components'],
    'node': ['version', 'archive_url', 'archive_sha256'],
    'pnpm': ['version', 'wrapper_url', 'wrapper_sha512', 'native_package',
             'native_url', 'native_sha512'],
    'typescript': ['version', 'wrapper_url', 'wrapper_sha512', 'native_package',
                   'native_url', 'native_sha512'],
    'esbuild': ['version', 'wrapper_url', 'wrapper_sha512', 'native_package',
                'native_url', 'native_sha512'],
    'electron': ['version', 'wrapper_url', 'wrapper_sha512', 'binary_url', 'binary_sha256'],
    'sqlite': ['version', 'version_number', 'source_id', 'amalgamation_url',
               'amalgamation_sha3_256', 'common_c_flags', 'macos_min_version'],
}


class Problem(Exception):
    pass


def fail(message: str) -> None:
    print(f'bootstrap: {message}', file=sys.stderr)
    raise SystemExit(2)


def load() -> dict:
    if not PINS.is_file():
        fail(f'missing {PINS}. The bootstrap scripts cannot run without it.')
    try:
        data = json.loads(PINS.read_text())
    except json.JSONDecodeError as error:
        fail(f'{PINS.name} is not valid JSON: {error}')
    if data.get('schema_version') != SUPPORTED_SCHEMA:
        fail(f"{PINS.name} schema_version {data.get('schema_version')!r} is not "
             f'supported (expected {SUPPORTED_SCHEMA!r}).')
    return data


def host_platform() -> str:
    system, machine = platform.system().lower(), platform.machine().lower()
    if system == 'linux' and machine == 'aarch64':
        machine = 'arm64'
    return f'{system}-{machine}'


def check_platform(data: dict) -> None:
    actual = host_platform()
    if actual not in SUPPORTED_PLATFORMS:
        fail(f'unsupported platform {actual}; no substitute toolchain is selected.')
    if actual != data.get('platform'):
        if actual not in data.get('platform_overrides', {}):
            fail(f'missing explicit acquisition profile for {actual}')



def tool(data: dict, name: str) -> dict:
    tools = data.get('tools', {})
    if name not in tools:
        fail(f'unknown tool {name!r}; known tools: {", ".join(sorted(tools))}')
    entry = dict(tools[name])
    entry.update(data.get('platform_overrides', {}).get(host_platform(), {}).get(name, {}))
    missing = [field for field in REQUIRED_FIELDS.get(name, []) if field not in entry]
    if missing:
        fail(f'pins.json tool {name!r} is missing required field(s): {", ".join(missing)}')
    return entry


def main(argv: list[str]) -> int:
    data = load()

    if argv[:1] == ['--platform']:
        check_platform(data)
        print(f'{host_platform()} supported')
        return 0

    if argv[:1] == ['--list']:
        for name in sorted(data.get('tools', {})):
            print(name)
        return 0

    check_platform(data)

    if argv[:2] == ['--tool-version', ''] or (len(argv) == 2 and argv[0] == '--tool-version'):
        print(tool(data, argv[1])['version'])
        return 0

    if argv[:1] == ['--field'] and len(argv) == 3:
        entry = tool(data, argv[1])
        field = argv[2]
        if field not in entry:
            fail(f'pins.json tool {argv[1]!r} has no field {field!r}')
        value = entry[field]
        if isinstance(value, list):
            print(' '.join(str(item) for item in value))
        else:
            print(value)
        return 0

    fail('usage: pins.py --platform | --list | --tool-version <tool> | --field <tool> <field>')
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
