import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterator, Sequence
from typing import AnyStr
from typing_extensions import deprecated

__all__ = ["escape", "glob", "iglob"]

if sys.version_info >= (3, 13):
    __all__ += ["translate"]

if sys.version_info >= (3, 10):
    @deprecated(
        "Deprecated since Python 3.10; will be removed in Python 3.15. Use `glob.glob()` with the *root_dir* argument instead."
    )
    def glob0(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...
    @deprecated(
        "Deprecated since Python 3.10; will be removed in Python 3.15. Use `glob.glob()` with the *root_dir* argument instead."
    )
    def glob1(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...

else:
    def glob0(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...
    def glob1(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...

if sys.version_info >= (3, 11):
    def glob(
        pathname: AnyStr,
        *,
        root_dir: StrOrBytesPath | None = None,
        dir_fd: int | None = None,
        recursive: bool = False,
        include_hidden: bool = False,
    ) -> list[AnyStr]: ...
    def iglob(
        pathname: AnyStr,
        *,
        root_dir: StrOrBytesPath | None = None,
        dir_fd: int | None = None,
        recursive: bool = False,
        include_hidden: bool = False,
    ) -> Iterator[AnyStr]: ...

elif sys.version_info >= (3, 10):
    def glob(
        pathname: AnyStr, *, root_dir: StrOrBytesPath | None = None, dir_fd: int | None = None, recursive: bool = False
    ) -> list[AnyStr]: ...
    def iglob(
        pathname: AnyStr, *, root_dir: StrOrBytesPath | None = None, dir_fd: int | None = None, recursive: bool = False
    ) -> Iterator[AnyStr]: ...

else:
    def glob(pathname: AnyStr, *, recursive: bool = False) -> list[AnyStr]: ...
    def iglob(pathname: AnyStr, *, recursive: bool = False) -> Iterator[AnyStr]: ...

def escape(pathname: AnyStr) -> AnyStr: ...
def has_magic(s: str | bytes) -> bool: ...  # undocumented

if sys.version_info >= (3, 13):
    def translate(
        pat: str, *, recursive: bool = False, include_hidden: bool = False, seps: Sequence[str] | None = None
    ) -> str: ...
