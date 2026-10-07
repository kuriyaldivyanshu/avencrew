"""Integrity regression tests; run with python3 -m unittest discover -s tools/bootstrap."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from rust_mirror import prepare


class RustMirrorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = self.root / 'channel-rust-1.99.0.toml'
        self.mirror = self.root / 'mirror'
        self.payload = b'verified component archive'
        self.digest = hashlib.sha256(self.payload).hexdigest()
        self.manifest.write_text('''manifest-version = "2"
date = "2026-10-01"
[profiles]
minimal = ["rustc", "rust-std", "cargo", "rust-mingw"]
[renames.rustfmt]
to = "rustfmt-preview"
''')
        with self.manifest.open('a') as manifest:
            for host in ('aarch64-apple-darwin', 'aarch64-unknown-linux-gnu'):
                manifest.write(f'[pkg.rust.target.{host}]\ncomponents = [\n')
                for name in ('rustc', 'rust-std', 'cargo'):
                    manifest.write(f'{{ pkg = "{name}", target = "{host}" }},\n')
                manifest.write(']\n')
            for name in ('rustc', 'rust-std', 'cargo', 'rustfmt-preview'):
                for host in ('aarch64-apple-darwin', 'aarch64-unknown-linux-gnu'):
                    manifest.write(f'[pkg.{name}.target.{host}]\navailable = true\n')
                    for prefix, extension in (('', 'gz'), ('xz_', 'xz'), ('zst_', 'zst')):
                        manifest.write(
                            f'{prefix}url = "https://static.rust-lang.org/dist/2026-10-01/'
                            f'{name}-{host}.tar.{extension}"\n{prefix}hash = "{self.digest}"\n'
                        )

    def download(self, args, **kwargs):
        Path(args[args.index('-o') + 1]).write_bytes(self.payload)

    def prepare(self, host='aarch64-apple-darwin'):
        prepare(self.manifest, self.mirror, host, ['rustfmt'])

    def test_both_hosts_profiles_renames_formats_and_exact_manifest(self):
        for host in ('aarch64-apple-darwin', 'aarch64-unknown-linux-gnu'):
            with self.subTest(host=host), patch('rust_mirror.subprocess.run', self.download):
                self.prepare(host)
                archives = list((self.mirror / 'dist/2026-10-01').glob(f'*{host}.tar.*'))
                self.assertEqual(len(archives), 12)
                self.assertTrue(all(path.read_bytes() == self.payload for path in archives))
                local = self.mirror / 'dist' / self.manifest.name
                self.assertEqual(local.read_bytes(), self.manifest.read_bytes())
                checksum = local.with_suffix('.toml.sha256').read_text().split()[0]
                self.assertEqual(checksum, hashlib.sha256(local.read_bytes()).hexdigest())
        with patch('rust_mirror.subprocess.run') as download:
            self.prepare()
            download.assert_not_called()

    def test_corrupt_download_stops_before_publishing_manifest(self):
        self.payload = b'tampered'
        with patch('rust_mirror.subprocess.run', self.download):
            with self.assertRaisesRegex(ValueError, 'integrity mismatch'):
                self.prepare()
        self.assertFalse((self.mirror / 'dist' / self.manifest.name).exists())
        self.assertEqual(list(self.mirror.rglob('*.partial')), [])

    def test_cached_archives_are_reverified(self):
        with patch('rust_mirror.subprocess.run', self.download):
            self.prepare()
        next(self.mirror.rglob('*.tar.xz')).write_bytes(b'tampered cache')
        with patch('rust_mirror.subprocess.run') as download:
            with self.assertRaisesRegex(ValueError, 'integrity mismatch'):
                self.prepare()
            download.assert_not_called()

    def test_unavailable_component_fails(self):
        self.manifest.write_text(self.manifest.read_text().replace('available = true', 'available = false'))
        with self.assertRaisesRegex(ValueError, 'unavailable'):
            self.prepare()

    def test_archive_url_cannot_escape_mirror_or_use_another_server(self):
        original = self.manifest.read_text()
        for url in ('https://example.com/dist/', 'https://static.rust-lang.org/dist/../',
                    'https://static.rust-lang.org/dist/%2e%2e/'):
            with self.subTest(url=url):
                self.manifest.write_text(original.replace('https://static.rust-lang.org/dist/', url))
                with self.assertRaisesRegex(ValueError, 'Invalid Rust archive'):
                    self.prepare()


if __name__ == '__main__':
    unittest.main()
