"""Python interface for the 'lsprof' profiler.
Compatible with the 'profile' module.
"""

import _lsprof
from _typeshed import StrOrBytesPath, Unused
from collections.abc import Callable, Mapping
from types import CodeType
from typing import Any, TypeVar
from typing_extensions import ParamSpec, Self, TypeAlias

__all__ = ["run", "runctx", "Profile"]

def run(statement: str, filename: str | None = None, sort: str | int = -1) -> None:
    """Run statement under profiler optionally saving results in filename

    This function takes a single argument that can be passed to the
    "exec" statement, and an optional file name.  In all cases this
    routine attempts to "exec" its first argument and gather profiling
    statistics from the execution. If no file name is present, then this
    function automatically prints a simple profiling report, sorted by the
    standard name string (file/line/function-name) that is presented in
    each line.
    """

def runctx(
    statement: str, globals: dict[str, Any], locals: Mapping[str, Any], filename: str | None = None, sort: str | int = -1
) -> None:
    """Run statement under profiler, supplying your own globals and locals,
    optionally saving results in filename.

    statement and filename have the same semantics as profile.run
    """

_T = TypeVar("_T")
_P = ParamSpec("_P")
_Label: TypeAlias = tuple[str, int, str]

class Profile(_lsprof.Profiler):
    """Profile(timer=None, timeunit=None, subcalls=True, builtins=True)

    Builds a profiler object using the specified timer function.
    The default timer is a fast built-in one based on real time.
    For custom timer functions returning integers, timeunit can
    be a float specifying a scale (i.e. how long each integer unit
    is, in seconds).
    """

    stats: dict[_Label, tuple[int, int, int, int, dict[_Label, tuple[int, int, int, int]]]]  # undocumented
    def print_stats(self, sort: str | int = -1) -> None: ...
    def dump_stats(self, file: StrOrBytesPath) -> None: ...
    def create_stats(self) -> None: ...
    def snapshot_stats(self) -> None: ...
    def run(self, cmd: str) -> Self: ...
    def runctx(self, cmd: str, globals: dict[str, Any], locals: Mapping[str, Any]) -> Self: ...
    def runcall(self, func: Callable[_P, _T], /, *args: _P.args, **kw: _P.kwargs) -> _T: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *exc_info: Unused) -> None: ...

def label(code: str | CodeType) -> _Label: ...  # undocumented
