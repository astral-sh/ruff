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
        frame: FrameType

    @dataclass(frozen=True, slots=True)
    class FutureCallGraph:
        future: Future[Any]
        call_stack: tuple[FrameCallGraphEntry, ...]
        awaited_by: tuple[FutureCallGraph, ...]

    @overload
    def capture_call_graph(future: None = None, /, *, depth: int = 1, limit: int | None = None) -> FutureCallGraph | None: ...
    @overload
    def capture_call_graph(future: Future[Any], /, *, depth: int = 1, limit: int | None = None) -> FutureCallGraph | None: ...
    def format_call_graph(future: Future[Any] | None = None, /, *, depth: int = 1, limit: int | None = None) -> str: ...
    def print_call_graph(
        future: Future[Any] | None = None, /, *, file: SupportsWrite[str] | None = None, depth: int = 1, limit: int | None = None
    ) -> None: ...
