"""Debugger basics"""

import sys
from _typeshed import ExcInfo, TraceFunction, Unused
from collections.abc import Callable, Iterable, Iterator, Mapping
from contextlib import contextmanager
from types import CodeType, FrameType, TracebackType
from typing import IO, Any, Final, Literal, SupportsInt, TypeVar
from typing_extensions import ParamSpec, TypeAlias

__all__ = ["BdbQuit", "Bdb", "Breakpoint"]

_T = TypeVar("_T")
_P = ParamSpec("_P")
_Backend: TypeAlias = Literal["settrace", "monitoring"]

# A union of code-object flags at runtime.
# The exact values of code-object flags are implementation details,
# so we don't include the value of this constant in the stubs.
GENERATOR_AND_COROUTINE_FLAGS: Final[int]

class BdbQuit(Exception):
    """Exception to give up completely."""

class Bdb:
    """Generic Python debugger base class.

    This class takes care of details of the trace facility;
    a derived class should implement user interaction.
    The standard debugger class (pdb.Pdb) is an example.

    The optional skip argument must be an iterable of glob-style
    module name patterns.  The debugger will not step into frames
    that originate in a module that matches one of these patterns.
    Whether a frame is considered to originate in a certain module
    is determined by the __name__ in the frame globals.
    """

    skip: set[str] | None
    breaks: dict[str, list[int]]
    fncache: dict[str, str]
    frame_returning: FrameType | None
    botframe: FrameType | None
    quitting: bool
    stopframe: FrameType | None
    returnframe: FrameType | None
    stoplineno: int
    if sys.version_info >= (3, 14):
        backend: _Backend
        def __init__(self, skip: Iterable[str] | None = None, backend: _Backend = "settrace") -> None: ...
    else:
        def __init__(self, skip: Iterable[str] | None = None) -> None: ...

    def canonic(self, filename: str) -> str:
        """Return canonical form of filename.

        For real filenames, the canonical form is a case-normalized (on
        case insensitive filesystems) absolute path.  'Filenames' with
        angle brackets, such as "<stdin>", generated in interactive
        mode, are returned unchanged.
        """

    def reset(self) -> None:
        """Set values of attributes as ready to start debugging."""
    if sys.version_info >= (3, 12):
        @contextmanager
        def set_enterframe(self, frame: FrameType) -> Iterator[None]: ...

    def trace_dispatch(self, frame: FrameType, event: str, arg: Any) -> TraceFunction:
        """Dispatch a trace function for debugged frames based on the event.

        This function is installed as the trace function for debugged
        frames. Its return value is the new trace function, which is
        usually itself. The default implementation decides how to
        dispatch a frame, depending on the type of event (passed in as a
        string) that is about to be executed.

        The event can be one of the following:
            line: A new line of code is going to be executed.
            call: A function is about to be called or another code block
                  is entered.
            return: A function or other code block is about to return.
            exception: An exception has occurred.
            c_call: A C function is about to be called.
            c_return: A C function has returned.
            c_exception: A C function has raised an exception.

        For the Python events, specialized functions (see the dispatch_*()
        methods) are called.  For the C events, no action is taken.

        The arg parameter depends on the previous event.
        """

    def dispatch_line(self, frame: FrameType) -> TraceFunction:
        """Invoke user function and return trace function for line event.

        If the debugger stops on the current line, invoke
        self.user_line(). Raise BdbQuit if self.quitting is set.
        Return self.trace_dispatch to continue tracing in this scope.
        """

    def dispatch_call(self, frame: FrameType, arg: None) -> TraceFunction:
        """Invoke user function and return trace function for call event.

        If the debugger stops on this function call, invoke
        self.user_call(). Raise BdbQuit if self.quitting is set.
        Return self.trace_dispatch to continue tracing in this scope.
        """

    def dispatch_return(self, frame: FrameType, arg: Any) -> TraceFunction:
        """Invoke user function and return trace function for return event.

        If the debugger stops on this function return, invoke
        self.user_return(). Raise BdbQuit if self.quitting is set.
        Return self.trace_dispatch to continue tracing in this scope.
        """

    def dispatch_exception(self, frame: FrameType, arg: ExcInfo) -> TraceFunction:
        """Invoke user function and return trace function for exception event.

        If the debugger stops on this exception, invoke
        self.user_exception(). Raise BdbQuit if self.quitting is set.
        Return self.trace_dispatch to continue tracing in this scope.
        """
    if sys.version_info >= (3, 13):
        def dispatch_opcode(self, frame: FrameType, arg: Unused) -> Callable[[FrameType, str, Any], TraceFunction]:
            """Invoke user function and return trace function for opcode event.
            If the debugger stops on the current opcode, invoke
            self.user_opcode(). Raise BdbQuit if self.quitting is set.
            Return self.trace_dispatch to continue tracing in this scope.

            Opcode event will always trigger the user callback. For now the only
            opcode event is from an inline set_trace() and we want to stop there
            unconditionally.
            """

    def is_skipped_module(self, module_name: str) -> bool:
        """Return True if module_name matches any skip pattern."""

    def stop_here(self, frame: FrameType) -> bool:
        """Return True if frame is below the starting frame in the stack."""

    def break_here(self, frame: FrameType) -> bool:
        """Return True if there is an effective breakpoint for this line.

        Check for line or function breakpoint and if in effect.
        Delete temporary breakpoints if effective() says to.
        """

    def do_clear(self, arg: Any) -> bool | None:
        """Remove temporary breakpoint.

        Must implement in derived classes or get NotImplementedError.
        """

    def break_anywhere(self, frame: FrameType) -> bool:
        """Return True if there is any breakpoint in that frame"""

    def user_call(self, frame: FrameType, argument_list: None) -> None:
        """Called if we might stop in a function."""

    def user_line(self, frame: FrameType) -> None:
        """Called when we stop or break at a line."""

    def user_return(self, frame: FrameType, return_value: Any) -> None:
        """Called when a return trap is set here."""

    def user_exception(self, frame: FrameType, exc_info: ExcInfo) -> None:
        """Called when we stop on an exception."""

    def set_until(self, frame: FrameType, lineno: int | None = None) -> None:
        """Stop when the line with the lineno greater than the current one is
        reached or when returning from current frame.
        """
    if sys.version_info >= (3, 13):
        def user_opcode(self, frame: FrameType) -> None:  # undocumented
            """Called when we are about to execute an opcode."""

    def set_step(self) -> None:
        """Stop after one line of code."""
    if sys.version_info >= (3, 13):
        def set_stepinstr(self) -> None:  # undocumented
            """Stop before the next instruction."""

    def set_next(self, frame: FrameType) -> None:
        """Stop on the next line in or below the given frame."""

    def set_return(self, frame: FrameType) -> None:
        """Stop when returning from the given frame."""

    def set_trace(self, frame: FrameType | None = None) -> None:
        """Start debugging from frame.

        If frame is not specified, debugging starts from caller's frame.
        """

    def set_continue(self) -> None:
        """Stop only at breakpoints or when finished.

        If there are no breakpoints, set the system trace function to None.
        """

    def set_quit(self) -> None:
        """Set quitting attribute to True.

        Raises BdbQuit exception in the next call to a dispatch_*() method.
        """

    def set_break(
        self, filename: str, lineno: int, temporary: bool = False, cond: str | None = None, funcname: str | None = None
    ) -> str | None:
        """Set a new breakpoint for filename:lineno.

        If lineno doesn't exist for the filename, return an error message.
        The filename should be in canonical form.
        """

    def clear_break(self, filename: str, lineno: int) -> str | None:
        """Delete breakpoints for filename:lineno.

        If no breakpoints were set, return an error message.
        """

    def clear_bpbynumber(self, arg: SupportsInt) -> str | None:
        """Delete a breakpoint by its index in Breakpoint.bpbynumber.

        If arg is invalid, return an error message.
        """

    def clear_all_file_breaks(self, filename: str) -> str | None:
        """Delete all breakpoints in filename.

        If none were set, return an error message.
        """

    def clear_all_breaks(self) -> str | None:
        """Delete all existing breakpoints.

        If none were set, return an error message.
        """

    def get_bpbynumber(self, arg: SupportsInt) -> Breakpoint:
        """Return a breakpoint by its index in Breakpoint.bybpnumber.

        For invalid arg values or if the breakpoint doesn't exist,
        raise a ValueError.
        """

    def get_break(self, filename: str, lineno: int) -> bool:
        """Return True if there is a breakpoint for filename:lineno."""

    def get_breaks(self, filename: str, lineno: int) -> list[Breakpoint]:
        """Return all breakpoints for filename:lineno.

        If no breakpoints are set, return an empty list.
        """

    def get_file_breaks(self, filename: str) -> list[int]:
        """Return all lines with breakpoints for filename.

        If no breakpoints are set, return an empty list.
        """

    def get_all_breaks(self) -> dict[str, list[int]]:
        """Return all breakpoints that are set."""

    def get_stack(self, f: FrameType | None, t: TracebackType | None) -> tuple[list[tuple[FrameType, int]], int]:
        """Return a list of (frame, lineno) in a stack trace and a size.

        List starts with original calling frame, if there is one.
        Size may be number of frames above or below f.
        """

    def format_stack_entry(self, frame_lineno: tuple[FrameType, int], lprefix: str = ": ") -> str:
        """Return a string with information about a stack entry.

        The stack entry frame_lineno is a (frame, lineno) tuple.  The
        return string contains the canonical filename, the function name
        or '<lambda>', the input arguments, the return value, and the
        line of code (if it exists).

        """

    def run(self, cmd: str | CodeType, globals: dict[str, Any] | None = None, locals: Mapping[str, Any] | None = None) -> None:
        """Debug a statement executed via the exec() function.

        globals defaults to __main__.dict; locals defaults to globals.
        """

    def runeval(self, expr: str, globals: dict[str, Any] | None = None, locals: Mapping[str, Any] | None = None) -> None:
        """Debug an expression executed via the eval() function.

        globals defaults to __main__.dict; locals defaults to globals.
        """

    def runctx(self, cmd: str | CodeType, globals: dict[str, Any] | None, locals: Mapping[str, Any] | None) -> None:
        """For backwards-compatibility.  Defers to run()."""

    def runcall(self, func: Callable[_P, _T], /, *args: _P.args, **kwds: _P.kwargs) -> _T | None:
        """Debug a single function call.

        Return the result of the function call.
        """
    if sys.version_info >= (3, 14):
        def start_trace(self) -> None: ...
        def stop_trace(self) -> None: ...
        def disable_current_event(self) -> None:
            """Disable the current event."""

        def restart_events(self) -> None:
            """Restart all events."""

class Breakpoint:
    """Breakpoint class.

    Implements temporary breakpoints, ignore counts, disabling and
    (re)-enabling, and conditionals.

    Breakpoints are indexed by number through bpbynumber and by
    the (file, line) tuple using bplist.  The former points to a
    single instance of class Breakpoint.  The latter points to a
    list of such instances since there may be more than one
    breakpoint per line.

    When creating a breakpoint, its associated filename should be
    in canonical form.  If funcname is defined, a breakpoint hit will be
    counted when the first line of that function is executed.  A
    conditional breakpoint always counts a hit.
    """

    next: int
    bplist: dict[tuple[str, int], list[Breakpoint]]
    bpbynumber: list[Breakpoint | None]

    funcname: str | None
    func_first_executable_line: int | None
    file: str
    line: int
    temporary: bool
    cond: str | None
    enabled: bool
    ignore: int
    hits: int
    number: int
    def __init__(
        self, file: str, line: int, temporary: bool = False, cond: str | None = None, funcname: str | None = None
    ) -> None: ...
    if sys.version_info >= (3, 11):
        @staticmethod
        def clearBreakpoints() -> None: ...

    def deleteMe(self) -> None:
        """Delete the breakpoint from the list associated to a file:line.

        If it is the last breakpoint in that position, it also deletes
        the entry for the file:line.
        """

    def enable(self) -> None:
        """Mark the breakpoint as enabled."""

    def disable(self) -> None:
        """Mark the breakpoint as disabled."""

    def bpprint(self, out: IO[str] | None = None) -> None:
        """Print the output of bpformat().

        The optional out argument directs where the output is sent
        and defaults to standard output.
        """

    def bpformat(self) -> str:
        """Return a string with information about the breakpoint.

        The information includes the breakpoint number, temporary
        status, file:line position, break condition, number of times to
        ignore, and number of times hit.

        """

def checkfuncname(b: Breakpoint, frame: FrameType) -> bool:
    """Return True if break should happen here.

    Whether a break should happen depends on the way that b (the breakpoint)
    was set.  If it was set via line number, check if b.line is the same as
    the one in the frame.  If it was set via function name, check if this is
    the right function and if it is on the first executable line.
    """

def effective(file: str, line: int, frame: FrameType) -> tuple[Breakpoint, bool] | tuple[None, None]:
    """Return (active breakpoint, delete temporary flag) or (None, None) as
    breakpoint to act upon.

    The "active breakpoint" is the first entry in bplist[line, file] (which
    must exist) that is enabled, for which checkfuncname is True, and that
    has neither a False condition nor a positive ignore count.  The flag,
    meaning that a temporary breakpoint should be deleted, is False only
    when the condiion cannot be evaluated (in which case, ignore count is
    ignored).

    If no such entry exists, then (None, None) is returned.
    """

def set_trace() -> None:
    """Start debugging with a Bdb instance from the caller's frame."""
