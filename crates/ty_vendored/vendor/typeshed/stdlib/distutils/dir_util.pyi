from _typeshed import StrOrBytesPath, StrPath
from collections.abc import Iterable
from typing import Literal

def mkpath(name: str, mode: int = 0o777, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0) -> list[str]: ...
def create_tree(
    base_dir: StrPath,
    files: Iterable[StrPath],
    mode: int = 0o777,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> None: ...
def copy_tree(
    src: StrPath,
    dst: str,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    preserve_symlinks: bool | Literal[0, 1] = 0,
    update: bool | Literal[0, 1] = 0,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> list[str]: ...
def remove_tree(directory: StrOrBytesPath, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0) -> None: ...
