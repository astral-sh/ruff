from _typeshed import BytesPath, StrOrBytesPath, StrPath
from collections.abc import Iterable
from typing import Literal, TypeVar, overload

_StrPathT = TypeVar("_StrPathT", bound=StrPath)
_BytesPathT = TypeVar("_BytesPathT", bound=BytesPath)

@overload
def copy_file(
    src: StrPath,
    dst: _StrPathT,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    update: bool | Literal[0, 1] = 0,
    link: str | None = None,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> tuple[_StrPathT | str, bool]: ...
@overload
def copy_file(
    src: BytesPath,
    dst: _BytesPathT,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    update: bool | Literal[0, 1] = 0,
    link: str | None = None,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> tuple[_BytesPathT | bytes, bool]: ...
@overload
def move_file(
    src: StrPath, dst: _StrPathT, verbose: bool | Literal[0, 1] = 0, dry_run: bool | Literal[0, 1] = 0
) -> _StrPathT | str: ...
@overload
def move_file(
    src: BytesPath, dst: _BytesPathT, verbose: bool | Literal[0, 1] = 0, dry_run: bool | Literal[0, 1] = 0
) -> _BytesPathT | bytes: ...
def write_file(filename: StrOrBytesPath, contents: Iterable[str]) -> None: ...
