"""Prepare isolated PTY copies; never modify the application's vendored source."""
import argparse, difflib, hashlib, json, shutil
from pathlib import Path

FLAGS = {'V1': 0x7, 'V2': 0x7, 'V3': 0x6, 'V4': 0xF}
LOAD = '''fn load_conpty() -> ConPtyFuncs {
    let library = std::env::var_os("SCROLL_CONPTY_DLL").expect("test DLL path required");
    assert!(Path::new(&library).is_absolute(), "absolute test DLL path required");
    ConPtyFuncs::open(Path::new(&library)).expect("requested test ConPTY library must load; no fallback")
}'''


def prepare(variant, output):
    host = Path(__file__).resolve().parent
    original = host.parents[2] / 'vendor/portable-pty'
    output = output.resolve()
    for protected in (original.resolve(), (host / 'src').resolve()):
        if output.is_relative_to(protected):
            raise ValueError('output must not be inside either copied source tree')
    output.mkdir(parents=True, exist_ok=False)
    vendor = output / 'portable-pty'
    shutil.copytree(original, vendor)
    generated = output / 'host'
    generated.mkdir()
    shutil.copytree(host / 'src', generated / 'src')
    for name in ['Cargo.lock', 'replay.ps1']:
        shutil.copy2(host / name, generated / name)
    manifest = (host / 'Cargo.toml').read_text().replace(
        'path = "../../../vendor/portable-pty"',
        'path = ' + json.dumps(vendor.resolve().as_posix(), ensure_ascii=False))
    assert str(vendor.resolve().as_posix()) in manifest
    (generated / 'Cargo.toml').write_text(manifest)
    path = vendor / 'src/win/psuedocon.rs'
    original_bytes = path.read_bytes()
    before = original_bytes.decode().replace("\r\n", "\n")
    start = before.index('fn load_conpty() -> ConPtyFuncs {')
    end = before.index('\nlazy_static!', start)
    after = before[:start] + LOAD + '\n' + before[end:]
    flags = '''PSUEDOCONSOLE_INHERIT_CURSOR
                    | PSEUDOCONSOLE_RESIZE_QUIRK
                    | PSEUDOCONSOLE_WIN32_INPUT_MODE'''
    assert after.count(flags) == 1
    if variant == 'V3':
        after = after.replace(flags, 'PSEUDOCONSOLE_RESIZE_QUIRK | PSEUDOCONSOLE_WIN32_INPUT_MODE')
    if variant == 'V4':
        after = after.replace(flags, flags + '\n                    | PSEUDOCONSOLE_PASSTHROUGH_MODE')
    generated_bytes = after.replace("\n", "\r\n").encode() if b"\r\n" in original_bytes else after.encode()
    path.write_bytes(generated_bytes)
    changes = [str(p.relative_to(original)) for p in original.rglob('*') if p.is_file()
               and p.read_bytes() != (vendor / p.relative_to(original)).read_bytes()]
    assert changes == ['src/win/psuedocon.rs']
    (output / 'vendor.diff').write_text(''.join(difflib.unified_diff(
        before.splitlines(True), after.splitlines(True), fromfile='original', tofile=variant)))
    (output / 'variant.json').write_text(json.dumps({
        'variant': variant, 'flags': FLAGS[variant], 'changed_copy_files': changes,
        'original_sha256': hashlib.sha256(original_bytes).hexdigest(),
        'generated_sha256': hashlib.sha256(generated_bytes).hexdigest(),
        'runtime_library': 'absolute SCROLL_CONPTY_DLL; failure never falls back',
    }, indent=2) + '\n')
    return generated


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('variant', choices=FLAGS)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    print(prepare(args.variant, args.output))
