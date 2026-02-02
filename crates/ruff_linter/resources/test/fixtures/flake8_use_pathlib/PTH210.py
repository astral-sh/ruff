from pathlib import (
    Path,
    PosixPath,
    PurePath,
    PurePosixPath,
    PureWindowsPath,
    WindowsPath,
)
import pathlib


path = Path()
posix_path: pathlib.PosixPath = PosixPath()
pure_path: PurePath = PurePath()
pure_posix_path = pathlib.PurePosixPath()
pure_windows_path: PureWindowsPath = pathlib.PureWindowsPath()
windows_path: pathlib.WindowsPath = pathlib.WindowsPath()


### Errors
path.with_suffix(".")
path.with_suffix("py")
path.with_suffix(r"s")
path.with_suffix(u'' "json")
path.with_suffix(suffix="js")

posix_path.with_suffix(".")
posix_path.with_suffix("py")
posix_path.with_suffix(r"s")
posix_path.with_suffix(u'' "json")
posix_path.with_suffix(suffix="js")

pure_path.with_suffix(".")
pure_path.with_suffix("py")
pure_path.with_suffix(r"s")
pure_path.with_suffix(u'' "json")
pure_path.with_suffix(suffix="js")

pure_posix_path.with_suffix(".")
pure_posix_path.with_suffix("py")
pure_posix_path.with_suffix(r"s")
pure_posix_path.with_suffix(u'' "json")
pure_posix_path.with_suffix(suffix="js")

pure_windows_path.with_suffix(".")
pure_windows_path.with_suffix("py")
pure_windows_path.with_suffix(r"s")
pure_windows_path.with_suffix(u'' "json")
pure_windows_path.with_suffix(suffix="js")

windows_path.with_suffix(".")
windows_path.with_suffix("py")
windows_path.with_suffix(r"s")
windows_path.with_suffix(u'' "json")
windows_path.with_suffix(suffix="js")

Path().with_suffix(".")
Path().with_suffix("py")
PosixPath().with_suffix("py")
PurePath().with_suffix("py")
PurePosixPath().with_suffix("py")
PureWindowsPath().with_suffix("py")
WindowsPath().with_suffix("py")

### No errors
path.with_suffix()
path.with_suffix('')
path.with_suffix(".py")
path.with_suffix("foo", "bar")
path.with_suffix(suffix)
path.with_suffix(f"oo")
path.with_suffix(b"ar")

posix_path.with_suffix()
posix_path.with_suffix('')
posix_path.with_suffix(".py")
posix_path.with_suffix("foo", "bar")
posix_path.with_suffix(suffix)
posix_path.with_suffix(f"oo")
posix_path.with_suffix(b"ar")

pure_path.with_suffix()
pure_path.with_suffix('')
pure_path.with_suffix(".py")
pure_path.with_suffix("foo", "bar")
pure_path.with_suffix(suffix)
pure_path.with_suffix(f"oo")
pure_path.with_suffix(b"ar")

pure_posix_path.with_suffix()
pure_posix_path.with_suffix('')
pure_posix_path.with_suffix(".py")
pure_posix_path.with_suffix("foo", "bar")
pure_posix_path.with_suffix(suffix)
pure_posix_path.with_suffix(f"oo")
pure_posix_path.with_suffix(b"ar")

pure_windows_path.with_suffix()
pure_windows_path.with_suffix('')
pure_windows_path.with_suffix(".py")
pure_windows_path.with_suffix("foo", "bar")
pure_windows_path.with_suffix(suffix)
pure_windows_path.with_suffix(f"oo")
pure_windows_path.with_suffix(b"ar")

windows_path.with_suffix()
windows_path.with_suffix('')
windows_path.with_suffix(".py")
windows_path.with_suffix("foo", "bar")
windows_path.with_suffix(suffix)
windows_path.with_suffix(f"oo")
windows_path.with_suffix(b"ar")
