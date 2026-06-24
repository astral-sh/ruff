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


### No Errors
path.with_suffix(".")
posix_path.with_suffix(".")
pure_path.with_suffix(".")
pure_posix_path.with_suffix(".")
pure_windows_path.with_suffix(".")
windows_path.with_suffix(".")
