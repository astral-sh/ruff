"""Terminal utilities."""

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
    IFLAG: Final[int]
    OFLAG: Final[int]
    CFLAG: Final[int]
    LFLAG: Final[int]
    ISPEED: Final[int]
    OSPEED: Final[int]
    CC: Final[int]
    def setraw(fd: _FD, when: int = 2) -> _ModeSetterReturn:
        """Put terminal into raw mode."""

    def setcbreak(fd: _FD, when: int = 2) -> _ModeSetterReturn:
        """Put terminal into cbreak mode."""
    if sys.version_info >= (3, 12):
        def cfmakeraw(mode: termios._Attr) -> None:
            """Make termios mode raw."""

        def cfmakecbreak(mode: termios._Attr) -> None:
            """Make termios mode cbreak."""
