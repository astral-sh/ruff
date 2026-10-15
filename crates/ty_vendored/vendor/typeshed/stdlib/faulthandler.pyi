import sys
from _typeshed import FileDescriptorLike

def cancel_dump_traceback_later() -> None: ...
def disable() -> None: ...

if sys.version_info >= (3, 15):
    def dump_traceback(
        file: FileDescriptorLike = sys.stderr, all_threads: bool = True, *, max_threads: int | None = None
    ) -> None: ...

else:
    def dump_traceback(file: FileDescriptorLike = sys.stderr, all_threads: bool = True) -> None: ...

if sys.version_info >= (3, 14):
    def dump_c_stack(file: FileDescriptorLike = sys.stderr) -> None: ...

if sys.version_info >= (3, 15):
    def dump_traceback_later(
        timeout: float,
        repeat: bool = False,
        file: FileDescriptorLike = sys.stderr,
        exit: bool = False,
        *,
        max_threads: int | None = None,
    ) -> None: ...

else:
    def dump_traceback_later(
        timeout: float, repeat: bool = False, file: FileDescriptorLike = sys.stderr, exit: bool = False
    ) -> None: ...

if sys.version_info >= (3, 15):
    def enable(
        file: FileDescriptorLike = sys.stderr, all_threads: bool = True, c_stack: bool = True, *, max_threads: int | None = None
    ) -> None: ...

elif sys.version_info >= (3, 14):
    def enable(file: FileDescriptorLike = sys.stderr, all_threads: bool = True, c_stack: bool = True) -> None: ...

else:
    def enable(file: FileDescriptorLike = sys.stderr, all_threads: bool = True) -> None: ...

def is_enabled() -> bool: ...

if sys.platform != "win32":
    if sys.version_info >= (3, 15):
        def register(
            signum: int,
            file: FileDescriptorLike = sys.stderr,
            all_threads: bool = True,
            chain: bool = False,
            *,
            max_threads: int | None = None,
        ) -> None: ...
    else:
        def register(
            signum: int, file: FileDescriptorLike = sys.stderr, all_threads: bool = True, chain: bool = False
        ) -> None: ...

    def unregister(signum: int, /) -> None: ...
