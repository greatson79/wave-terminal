"""Run actual preparation in retained CRLF trees with Windows path/encoding semantics."""
import importlib.util
import os
from pathlib import Path, PureWindowsPath
import shutil
import tomllib
import unittest
from unittest.mock import patch
import uuid

HOST = Path(__file__).resolve().parents[1]
ROOT = HOST.parents[2]
SOURCE = Path(os.environ.get('SCROLL_BUILDER_SOURCE', str(HOST / 'build_variant.py')))
PROOF = Path(os.environ['SCROLL_PREP_PROOF_DIR'])


class WindowsRelative:
    def __init__(self, value):
        self.windows = PureWindowsPath(value.as_posix())
    def __str__(self):
        return str(self.windows)
    def __fspath__(self):
        return self.windows.as_posix()
    def as_posix(self):
        return self.windows.as_posix()


class PrepareWindows(unittest.TestCase):
    def prepare_case(self, variant, ansi_defaults):
        folder = PROOF / ('windows-\ud55c\uae00-' + str(uuid.uuid4()))
        fake_host = folder / 'repo/scripts/probes/scroll-history'
        fake_host.mkdir(parents=True)
        shutil.copytree(ROOT / 'vendor/portable-pty', folder / 'repo/vendor/portable-pty')
        shutil.copytree(HOST / 'src', fake_host / 'src')
        for name in ['Cargo.toml', 'Cargo.lock', 'replay.ps1']:
            shutil.copy2(HOST / name, fake_host / name)
        original = folder / 'repo/vendor/portable-pty/src/win/psuedocon.rs'
        original.write_bytes(original.read_bytes().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n'))
        before = original.read_bytes()
        spec = importlib.util.spec_from_file_location('builder_' + uuid.uuid4().hex, SOURCE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.__file__ = str(fake_host / 'build_variant.py')
        native_relative, native_write, native_read = Path.relative_to, Path.write_text, Path.read_text
        def windows_relative(path, *args, **kwargs):
            return WindowsRelative(native_relative(path, *args, **kwargs))
        def windows_write(path, data, **kwargs):
            if ansi_defaults and not kwargs.get('encoding'):
                kwargs['encoding'] = 'cp1252'
            return native_write(path, data, **kwargs)
        def windows_read(path, **kwargs):
            if ansi_defaults and not kwargs.get('encoding'):
                kwargs['encoding'] = 'cp1252'
            return native_read(path, **kwargs)
        output = folder / 'variant'
        with patch.object(Path, 'relative_to', windows_relative), patch.object(Path, 'write_text', windows_write), patch.object(Path, 'read_text', windows_read):
            generated = module.prepare(variant, output)
        self.assertEqual(original.read_bytes(), before)
        changed = (output / 'portable-pty/src/win/psuedocon.rs').read_bytes()
        self.assertIn(b'\r\n', changed)
        self.assertNotIn(b'\n', changed.replace(b'\r\n', b''))
        import json, hashlib
        info = json.loads((output / 'variant.json').read_text(encoding='utf-8'))
        self.assertEqual(info['changed_copy_files'], ['src/win/psuedocon.rs'])
        self.assertEqual(info['flags'], {'V1':7,'V2':7,'V3':6,'V4':15}[variant])
        self.assertEqual(info['original_sha256'], hashlib.sha256(before).hexdigest())
        self.assertEqual(info['generated_sha256'], hashlib.sha256(changed).hexdigest())
        manifest = tomllib.loads((generated / 'Cargo.toml').read_text(encoding='utf-8'))
        self.assertEqual(manifest['dependencies']['portable-pty']['path'], (output / 'portable-pty').as_posix())
        self.assertEqual((generated / 'Cargo.lock').read_bytes(), (HOST / 'Cargo.lock').read_bytes())
        print(f'PASS {variant} Windows-backslash-relative + CRLF + cp1252-default={ansi_defaults}', flush=True)

    def test_windows_relative_paths_and_crlf(self):
        for variant in ['V1','V2','V3','V4']:
            with self.subTest(variant=variant): self.prepare_case(variant, False)

    def test_windows_relative_crlf_and_ansi_default_encoding(self):
        for variant in ['V1','V2','V3','V4']:
            with self.subTest(variant=variant): self.prepare_case(variant, True)


if __name__ == '__main__':
    unittest.main()
