"""Class for profiling Python code."""

from _typeshed import StrOrBytesPath
from collections.abc import Callable, Mapping
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

class Profile:
    """Profiler class.

    self.cur is always a tuple.  Each such tuple corresponds to a stack
    frame that is currently active (self.cur[-2]).  The following are the
    definitions of its members.  We use this external "parallel stack" to
    avoid contaminating the program that we are profiling. (old profiler
    used to write into the frames local dictionary!!) Derived classes
    can change the definition of some entries, as long as they leave
    [-2:] intact (frame and previous tuple).  In case an internal error is
    detected, the -3 element is used as the function name.

    [ 0] = Time that needs to be charged to the parent frame's function.
           It is used so that a function call will not have to access the
           timing data for the parent frame.
    [ 1] = Total time spent in this frame's function, excluding time in
           subfunctions (this latter is tallied in cur[2]).
    [ 2] = Total time spent in subfunctions, excluding time executing the
           frame's function (this latter is tallied in cur[1]).
    [-3] = Name of the function that corresponds to this frame.
    [-2] = Actual frame that we correspond to (used to sync exception handling).
    [-1] = Our parent 6-tuple (corresponds to frame.f_back).

    Timing data for each function is stored as a 5-tuple in the dictionary
    self.timings[].  The index is always the name stored in self.cur[-3].
    The following are the definitions of the members:

    [0] = The number of times this function was called, not counting direct
          or indirect recursion,
    [1] = Number of times this function appears on the stack, minus one
    [2] = Total time spent internal to this function
    [3] = Cumulative time that this function was present on the stack.  In
          non-recursive functions, this is the total execution time from start
          to finish of each invocation of a function, including time spent in
          all subfunctions.
    [4] = A dictionary indicating for each function name, the number of times
          it was called by us.
    """

    bias: int
    stats: dict[_Label, tuple[int, int, int, int, dict[_Label, tuple[int, int, int, int]]]]  # undocumented
    def __init__(self, timer: Callable[[], float] | None = None, bias: int | None = None) -> None: ...
    def set_cmd(self, cmd: str) -> None: ...
    def simulate_call(self, name: str) -> None: ...
    def simulate_cmd_complete(self) -> None: ...
    def print_stats(self, sort: str | int = -1) -> None: ...
    def dump_stats(self, file: StrOrBytesPath) -> None: ...
    def create_stats(self) -> None: ...
    def snapshot_stats(self) -> None: ...
    def run(self, cmd: str) -> Self: ...
    def runctx(self, cmd: str, globals: dict[str, Any], locals: Mapping[str, Any]) -> Self: ...
    def runcall(self, func: Callable[_P, _T], /, *args: _P.args, **kw: _P.kwargs) -> _T: ...
    def calibrate(self, m: int, verbose: int = 0) -> float: ...
