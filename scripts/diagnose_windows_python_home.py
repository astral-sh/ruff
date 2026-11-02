# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Trace the Windows path lookups that ty performs on a venv's Python home."""

from __future__ import annotations

import ctypes
import os
import subprocess
import sys
from ctypes import wintypes
from pathlib import Path

if sys.platform == "win32":
    from ctypes import WinDLL, WinError, get_last_error


def configure_kernel32():
    kernel = WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    ]
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.GetFinalPathNameByHandleW.argtypes = [
        wintypes.HANDLE,
        wintypes.LPWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
    ]
    kernel.GetFinalPathNameByHandleW.restype = wintypes.DWORD
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    return kernel


def probe_handle(kernel, path: str, *, follow: bool) -> None:
    # Rust's std::fs::canonicalize opens the directory with no requested access,
    # all three share modes, OPEN_EXISTING, and FILE_FLAG_BACKUP_SEMANTICS.
    flags = 0x02000000  # FILE_FLAG_BACKUP_SEMANTICS
    if not follow:
        flags |= 0x00200000  # FILE_FLAG_OPEN_REPARSE_POINT
    label = "follow" if follow else "OPEN_REPARSE_POINT"
    handle = kernel.CreateFileW(path, 0, 7, None, 3, flags, None)
    if handle == wintypes.HANDLE(-1).value:
        print(f"  CreateFileW({label}): {WinError(get_last_error())}")
        return

    print(f"  CreateFileW({label}): OK")
    try:
        for name, option in [("NORMALIZED", 0), ("OPENED", 8)]:
            buffer = ctypes.create_unicode_buffer(32768)
            length = kernel.GetFinalPathNameByHandleW(
                handle, buffer, len(buffer), option
            )
            if not length:
                print(
                    f"  GetFinalPathNameByHandleW({name}): {WinError(get_last_error())}"
                )
            elif length >= len(buffer):
                print(
                    f"  GetFinalPathNameByHandleW({name}): requires {length} characters"
                )
            else:
                print(f"  GetFinalPathNameByHandleW({name}): {buffer.value!r}")
    finally:
        kernel.CloseHandle(handle)


def main() -> None:
    if sys.platform != "win32":
        sys.exit("This diagnostic requires Windows.")
    config = Path(sys.argv[1]).read_text(encoding="utf-8")
    print(f"{sys.argv[1]}\n{config}", flush=True)
    settings = {}
    for line in config.splitlines():
        key, _, value = line.partition("=")
        settings[key.strip()] = value.strip()
    candidates = []
    home = settings.get("home")
    if home:
        candidates.extend([home, str(Path(home).parent)])
        try:
            candidates.append(os.path.join(os.path.dirname(home), os.readlink(home)))
        except OSError as error:
            print(f"readlink({home!r}): {error}")
    else:
        print("No home in pyvenv.cfg")

    # Include the concrete installation even if the minor-version junction has
    # disappeared, or the recorded home only contains a prefix of the real path.
    if install_dir := os.getenv("UV_PYTHON_INSTALL_DIR"):
        try:
            candidates.extend(
                str(entry)
                for entry in Path(install_dir).iterdir()
                if entry.name.startswith("cpython-")
            )
        except OSError as error:
            print(f"listdir({install_dir!r}): {error}")

    kernel = configure_kernel32()
    for path in dict.fromkeys(candidates):
        print(f"\nPath: {path!r}", flush=True)
        try:
            metadata = os.lstat(path)
            print(
                f"  attributes={metadata.st_file_attributes:#x}, reparse_tag={metadata.st_reparse_tag:#x}"
            )
        except OSError as error:
            print(f"  lstat: {error}")
        probe_handle(kernel, path, follow=True)
        probe_handle(kernel, path, follow=False)
        sys.stdout.flush()
        subprocess.run(["icacls", path, "/L"], check=False)
    if home:
        print(f"\nTrailing separator on home: {home!r}")
        probe_handle(kernel, os.path.join(home, ""), follow=True)


if __name__ == "__main__":
    main()
