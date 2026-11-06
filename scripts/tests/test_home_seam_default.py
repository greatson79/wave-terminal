#!/usr/bin/env python3
"""Check a default-feature binary's read-only home probe; CI uses a release-profile binary."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid


def real_profile():
    if os.name != "nt":
        return Path.home()

    class Guid(ctypes.Structure):
        _fields_ = [("data1", ctypes.c_uint32), ("data2", ctypes.c_uint16),
                    ("data3", ctypes.c_uint16), ("data4", ctypes.c_ubyte * 8)]

    folder_id = Guid.from_buffer_copy(uuid.UUID("5e6c858f-0e22-4760-9afe-ea3317b67173").bytes_le)
    path_pointer = ctypes.c_void_p()
    shell = ctypes.WinDLL("shell32", use_last_error=True)
    query = shell.SHGetKnownFolderPath
    query.argtypes = [ctypes.POINTER(Guid), ctypes.c_uint32, ctypes.c_void_p,
                      ctypes.POINTER(ctypes.c_void_p)]
    query.restype = ctypes.c_long
    free = ctypes.WinDLL("ole32").CoTaskMemFree
    free.argtypes = [ctypes.c_void_p]
    free.restype = None
    try:
        assert query(ctypes.byref(folder_id), 0, None, ctypes.byref(path_pointer)) == 0, "OS profile query failed"
        assert path_pointer.value, "OS returned an empty profile pointer"
        return Path(ctypes.wstring_at(path_pointer.value))
    finally:
        if path_pointer.value:
            free(path_pointer)


def normalized(path):
    return os.path.normcase(os.path.normpath(str(path)))


def check(binary):
    expected = real_profile()
    with tempfile.TemporaryDirectory(prefix="cys-default-home 최형석 ") as directory:
        temporary = Path(directory)
        environment = dict(os.environ)
        environment["CYS_HOME"] = str(temporary)
        result = subprocess.run([str(binary.resolve()), "phoenix-identity"],
                                env=environment, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, encoding="utf-8", timeout=20)
        assert result.returncode == 0, "read-only identity probe failed"
        assert not result.stderr, "read-only identity probe emitted unexpected stderr"
        identity = json.loads(result.stdout)
        assert isinstance(identity.get("home_dir"), str), "identity probe must include home_dir"
        observed = Path(identity["home_dir"])
        assert normalized(observed) == normalized(expected), "default-feature binary did not resolve the OS profile"
        assert normalized(observed) != normalized(temporary), "default-feature binary honored CYS_HOME"
        assert not list(temporary.rglob("*")), "read-only probe wrote under CYS_HOME"
    print("PASS default home resolution ignores CYS_HOME; temporary directory stayed empty")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    check(args.binary)
