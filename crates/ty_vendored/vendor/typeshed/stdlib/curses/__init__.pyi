import sys
from _curses import *
from _curses import window as window
from _typeshed import structseq
from collections.abc import Callable
from typing import Final, TypeVar, final, type_check_only
from typing_extensions import Concatenate, ParamSpec

# NOTE: The _curses module is ordinarily only available on Unix, but the
# windows-curses package makes it available on Windows as well with the same
# contents.

_T = TypeVar("_T")
_P = ParamSpec("_P")

# available after calling `curses.initscr()`
LINES: int
COLS: int

# available after calling `curses.start_color()`
COLORS: int
COLOR_PAIRS: int

def wrapper(func: Callable[Concatenate[window, _P], _T], /, *arg: _P.args, **kwds: _P.kwargs) -> _T: ...

# At runtime this class is unexposed and calls itself curses.ncurses_version.
# That name would conflict with the actual curses.ncurses_version, which is
# an instance of this class.
@final
@type_check_only
class _ncurses_version(structseq[int], tuple[int, int, int]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("major", "minor", "patch")

    @property
    def major(self) -> int: ...
    @property
    def minor(self) -> int: ...
    @property
    def patch(self) -> int: ...
