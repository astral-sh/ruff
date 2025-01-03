from pathlib import (
    Path,
    PosixPath,
    PurePath,
    PurePosixPath,
    PureWindowsPath,
    WindowsPath,
)


def test_path(p: Path) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")


def test_posix_path(p: PosixPath) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")


def test_pure_path(p: PurePath) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")


def test_pure_posix_path(p: PurePosixPath) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")


def test_pure_windows_path(p: PureWindowsPath) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")


def test_windows_path(p: WindowsPath) -> None:
    ## Errors
    p.with_suffix(".")
    p.with_suffix("py")
    p.with_suffix(r"s")
    p.with_suffix(u'' "json")
    p.with_suffix(suffix="js")

    ## No errors
    p.with_suffix()
    p.with_suffix('')
    p.with_suffix(".py")
    p.with_suffix("foo", "bar")
    p.with_suffix(suffix)
    p.with_suffix(f"oo")
    p.with_suffix(b"ar")
