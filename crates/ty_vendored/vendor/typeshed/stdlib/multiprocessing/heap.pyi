import sys
from collections.abc import Callable
from mmap import mmap
from multiprocessing import popen_forkserver, popen_spawn_posix, resource_sharer
from typing import Protocol, TypeAlias, type_check_only

__all__ = ["BufferWrapper"]

class Arena:
    size: int
    buffer: mmap
    if sys.platform == "win32":
        name: str
        def __init__(self, size: int) -> None: ...
    else:
        fd: int
        def __init__(self, size: int, fd: int = -1) -> None: ...

_Block: TypeAlias = tuple[Arena, int, int]

if sys.platform != "win32":
    @type_check_only
    class _SupportsDetach(Protocol):
        def detach(self) -> int: ...

    def reduce_arena(
        a: Arena,
    ) -> tuple[
        Callable[[int, _SupportsDetach], Arena],
        tuple[int, popen_forkserver._DupFd | popen_spawn_posix._DupFd | resource_sharer.DupFd],
    ]: ...
    def rebuild_arena(size: int, dupfd: _SupportsDetach) -> Arena: ...

class Heap:
    def __init__(self, size: int = ...) -> None: ...
    def free(self, block: _Block) -> None: ...
    def malloc(self, size: int) -> _Block: ...

class BufferWrapper:
    def __init__(self, size: int) -> None: ...
    def create_memoryview(self) -> memoryview: ...
