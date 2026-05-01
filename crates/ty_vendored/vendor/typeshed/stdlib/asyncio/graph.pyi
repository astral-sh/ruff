"""Introspection utils for tasks call graphs."""

import sys
from _typeshed import SupportsWrite
from asyncio import Future
from dataclasses import dataclass
from types import FrameType
from typing import Any, overload

if sys.version_info >= (3, 14):
    __all__ = ("capture_call_graph", "format_call_graph", "print_call_graph", "FrameCallGraphEntry", "FutureCallGraph")

    @dataclass(frozen=True, slots=True)
    class FrameCallGraphEntry:
        """FrameCallGraphEntry(frame: frame)"""

        frame: FrameType

    @dataclass(frozen=True, slots=True)
    class FutureCallGraph:
        """FutureCallGraph(future: _asyncio.Future, call_stack: tuple['FrameCallGraphEntry', ...], awaited_by: tuple['FutureCallGraph', ...])"""

        future: Future[Any]
        call_stack: tuple[FrameCallGraphEntry, ...]
        awaited_by: tuple[FutureCallGraph, ...]

    @overload
    def capture_call_graph(future: None = None, /, *, depth: int = 1, limit: int | None = None) -> FutureCallGraph | None:
        """Capture the async call graph for the current task or the provided Future.

        The graph is represented with three data structures:

        * FutureCallGraph(future, call_stack, awaited_by)

          Where 'future' is an instance of asyncio.Future or asyncio.Task.

          'call_stack' is a tuple of FrameGraphEntry objects.

          'awaited_by' is a tuple of FutureCallGraph objects.

        * FrameCallGraphEntry(frame)

          Where 'frame' is a frame object of a regular Python function
          in the call stack.

        Receives an optional 'future' argument. If not passed,
        the current task will be used. If there's no current task, the function
        returns None.

        If "capture_call_graph()" is introspecting *the current task*, the
        optional keyword-only 'depth' argument can be used to skip the specified
        number of frames from top of the stack.

        If the optional keyword-only 'limit' argument is provided, each call stack
        in the resulting graph is truncated to include at most ``abs(limit)``
        entries. If 'limit' is positive, the entries left are the closest to
        the invocation point. If 'limit' is negative, the topmost entries are
        left. If 'limit' is omitted or None, all entries are present.
        If 'limit' is 0, the call stack is not captured at all, only
        "awaited by" information is present.
        """

    @overload
    def capture_call_graph(future: Future[Any], /, *, depth: int = 1, limit: int | None = None) -> FutureCallGraph | None: ...
    def format_call_graph(future: Future[Any] | None = None, /, *, depth: int = 1, limit: int | None = None) -> str:
        """Return the async call graph as a string for `future`.

        If `future` is not provided, format the call graph for the current task.
        """

    def print_call_graph(
        future: Future[Any] | None = None, /, *, file: SupportsWrite[str] | None = None, depth: int = 1, limit: int | None = None
    ) -> None:
        """Print the async call graph for the current task or the provided Future."""
