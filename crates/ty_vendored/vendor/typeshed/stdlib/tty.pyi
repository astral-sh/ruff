import sys
import termios
from typing import IO, Final
from typing_extensions import TypeAlias

if sys.platform != "win32":
    __all__ = ["setraw", "setcbreak"]
    if sys.version_info >= (3, 12):
        __all__ += ["cfmakeraw", "cfmakecbreak"]

        _ModeSetterReturn: TypeAlias = termios._AttrReturn
    else:
        _ModeSetterReturn: TypeAlias = None

    _FD: TypeAlias = int | IO[str]

    # XXX: Undocumented integer constants
    IFLAG: Final = 0
    OFLAG: Final = 1
    CFLAG: Final = 2
    LFLAG: Final = 3
    ISPEED: Final = 4
    OSPEED: Final = 5
    CC: Final = 6
    def setraw(fd: _FD, when: int = 2) -> _ModeSetterReturn: ...
    def setcbreak(fd: _FD, when: int = 2) -> _ModeSetterReturn: ...

    if sys.version_info >= (3, 12):
        def cfmakeraw(mode: termios._Attr) -> None: ...
        def cfmakecbreak(mode: termios._Attr) -> None: ...
