from _curses import *
from _curses import window as window
from collections.abc import Callable
from typing import TypeVar
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

# typeshed used the name _CursesWindow for the underlying C class before
# it was mapped to the name 'window' in 3.8.
# Kept here as a legacy alias in case any third-party code is relying on it.
_CursesWindow = window
