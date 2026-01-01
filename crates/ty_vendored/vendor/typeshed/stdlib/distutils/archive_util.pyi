from _typeshed import StrOrBytesPath, StrPath
from typing import Literal, overload

@overload
def make_archive(
    base_name: str,
    format: str,
    root_dir: StrOrBytesPath | None = None,
    base_dir: str | None = None,
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
    owner: str | None = None,
    group: str | None = None,
) -> str: ...
@overload
def make_archive(
    base_name: StrPath,
    format: str,
    root_dir: StrOrBytesPath,
    base_dir: str | None = None,
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
    owner: str | None = None,
    group: str | None = None,
) -> str: ...
def make_tarball(
    base_name: str,
    base_dir: StrPath,
    compress: str | None = "gzip",
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
    owner: str | None = None,
    group: str | None = None,
) -> str: ...
def make_zipfile(base_name: str, base_dir: str, verbose: bool | Literal[0, 1] = 0, dry_run: bool | Literal[0, 1] = 0) -> str: ...
