import sys
from collections.abc import Callable, Iterable
from typing import Final
from typing_extensions import TypeAlias, deprecated

if sys.platform != "win32":
    __all__ = ["openpty", "fork", "spawn"]
    _Reader: TypeAlias = Callable[[int], bytes]

    STDIN_FILENO: Final = 0
    STDOUT_FILENO: Final = 1
    STDERR_FILENO: Final = 2

    CHILD: Final = 0
    def openpty() -> tuple[int, int]: ...

    if sys.version_info < (3, 14):
        if sys.version_info >= (3, 12):
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14. Use `openpty()` instead.")
            def master_open() -> tuple[int, str]: ...
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14. Use `openpty()` instead.")
            def slave_open(tty_name: str) -> int: ...
        else:
            def master_open() -> tuple[int, str]: ...
            def slave_open(tty_name: str) -> int: ...

    def fork() -> tuple[int, int]: ...
    def spawn(argv: str | Iterable[str], master_read: _Reader = ..., stdin_read: _Reader = ...) -> int: ...
