import os
import sys
from _typeshed import ReadableBuffer, Unused
from collections.abc import Iterator
from typing import Final, Literal, NoReturn, overload
from typing_extensions import Self, disjoint_base

ACCESS_DEFAULT: Final = 0
ACCESS_READ: Final = 1
ACCESS_WRITE: Final = 2
ACCESS_COPY: Final = 3

ALLOCATIONGRANULARITY: Final[int]

if sys.platform == "linux":
    MAP_DENYWRITE: Final[int]
    MAP_EXECUTABLE: Final[int]
    if sys.version_info >= (3, 10):
        MAP_POPULATE: Final[int]
if sys.version_info >= (3, 11) and sys.platform != "win32" and sys.platform != "darwin":
    MAP_STACK: Final[int]

if sys.platform != "win32":
    MAP_ANON: Final[int]
    MAP_ANONYMOUS: Final[int]
    MAP_PRIVATE: Final[int]
    MAP_SHARED: Final[int]
    PROT_EXEC: Final[int]
    PROT_READ: Final[int]
    PROT_WRITE: Final[int]

PAGESIZE: Final[int]

@disjoint_base
class mmap:
    if sys.platform == "win32":
        def __new__(self, fileno: int, length: int, tagname: str | None = None, access: int = 0, offset: int = 0) -> Self: ...
    else:
        if sys.version_info >= (3, 13):
            def __new__(
                cls,
                fileno: int,
                length: int,
                flags: int = ...,
                prot: int = ...,
                access: int = 0,
                offset: int = 0,
                *,
                trackfd: bool = True,
            ) -> Self: ...
        else:
            def __new__(
                cls, fileno: int, length: int, flags: int = ..., prot: int = ..., access: int = 0, offset: int = 0
            ) -> Self: ...

    def close(self) -> None: ...
    def flush(self, offset: int = 0, size: int = ...) -> None: ...
    def move(self, dest: int, src: int, count: int) -> None: ...
    def read_byte(self) -> int: ...
    def readline(self) -> bytes: ...
    def resize(self, newsize: int) -> None: ...
    if sys.platform != "win32":
        def seek(self, pos: int, whence: Literal[0, 1, 2, 3, 4] = os.SEEK_SET) -> None: ...
    else:
        def seek(self, pos: int, whence: Literal[0, 1, 2] = os.SEEK_SET) -> None: ...

    def size(self) -> int: ...
    def tell(self) -> int: ...
    def write_byte(self, byte: int) -> None: ...
    def __len__(self) -> int: ...
    closed: bool
    if sys.platform != "win32":
        def madvise(self, option: int, start: int = 0, length: int = ...) -> None: ...

    def find(self, sub: ReadableBuffer, start: int = ..., stop: int = ...) -> int: ...
    def rfind(self, sub: ReadableBuffer, start: int = ..., stop: int = ...) -> int: ...
    def read(self, n: int | None = None) -> bytes: ...
    def write(self, bytes: ReadableBuffer) -> int: ...
    @overload
    def __getitem__(self, key: int, /) -> int: ...
    @overload
    def __getitem__(self, key: slice, /) -> bytes: ...
    def __delitem__(self, key: int | slice, /) -> NoReturn: ...
    @overload
    def __setitem__(self, key: int, value: int, /) -> None: ...
    @overload
    def __setitem__(self, key: slice, value: ReadableBuffer, /) -> None: ...
    # Doesn't actually exist, but the object actually supports "in" because it has __getitem__,
    # so we claim that there is also a __contains__ to help type checkers.
    def __contains__(self, o: object, /) -> bool: ...
    # Doesn't actually exist, but the object is actually iterable because it has __getitem__ and __len__,
    # so we claim that there is also an __iter__ to help type checkers.
    def __iter__(self) -> Iterator[int]: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def __buffer__(self, flags: int, /) -> memoryview: ...
    def __release_buffer__(self, buffer: memoryview, /) -> None: ...
    if sys.version_info >= (3, 13):
        def seekable(self) -> Literal[True]: ...

if sys.platform != "win32":
    MADV_NORMAL: Final[int]
    MADV_RANDOM: Final[int]
    MADV_SEQUENTIAL: Final[int]
    MADV_WILLNEED: Final[int]
    MADV_DONTNEED: Final[int]
    MADV_FREE: Final[int]

if sys.platform == "linux":
    MADV_REMOVE: Final[int]
    MADV_DONTFORK: Final[int]
    MADV_DOFORK: Final[int]
    MADV_HWPOISON: Final[int]
    MADV_MERGEABLE: Final[int]
    MADV_UNMERGEABLE: Final[int]
    # Seems like this constant is not defined in glibc.
    # See https://github.com/python/typeshed/pull/5360 for details
    # MADV_SOFT_OFFLINE: Final[int]
    MADV_HUGEPAGE: Final[int]
    MADV_NOHUGEPAGE: Final[int]
    MADV_DONTDUMP: Final[int]
    MADV_DODUMP: Final[int]

# This Values are defined for FreeBSD but type checkers do not support conditions for these
if sys.platform != "linux" and sys.platform != "darwin" and sys.platform != "win32":
    MADV_NOSYNC: Final[int]
    MADV_AUTOSYNC: Final[int]
    MADV_NOCORE: Final[int]
    MADV_CORE: Final[int]
    MADV_PROTECT: Final[int]

if sys.version_info >= (3, 10) and sys.platform == "darwin":
    MADV_FREE_REUSABLE: Final[int]
    MADV_FREE_REUSE: Final[int]

if sys.version_info >= (3, 13) and sys.platform != "win32":
    MAP_32BIT: Final[int]

if sys.version_info >= (3, 13) and sys.platform == "darwin":
    MAP_NORESERVE: Final = 64
    MAP_NOEXTEND: Final = 256
    MAP_HASSEMAPHORE: Final = 512
    MAP_NOCACHE: Final = 1024
    MAP_JIT: Final = 2048
    MAP_RESILIENT_CODESIGN: Final = 8192
    MAP_RESILIENT_MEDIA: Final = 16384
    MAP_TRANSLATED_ALLOW_EXECUTE: Final = 131072
    MAP_UNIX03: Final = 262144
    MAP_TPRO: Final = 524288

if sys.version_info >= (3, 13) and sys.platform == "linux":
    MAP_NORESERVE: Final = 16384
