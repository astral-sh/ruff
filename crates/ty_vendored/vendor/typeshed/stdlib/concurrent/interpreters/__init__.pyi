"""Subinterpreters High Level Module."""

import sys
import threading
import types
from collections.abc import Callable
from typing import Any, Literal, TypeVar
from typing_extensions import ParamSpec, Self

if sys.version_info >= (3, 13):  # needed to satisfy pyright checks for Python <3.13
    from _interpreters import (
        InterpreterError as InterpreterError,
        InterpreterNotFoundError as InterpreterNotFoundError,
        NotShareableError as NotShareableError,
        _SharedDict,
        _Whence,
        is_shareable as is_shareable,
    )

    from ._queues import Queue as Queue, QueueEmpty as QueueEmpty, QueueFull as QueueFull, create as create_queue

    __all__ = [
        "ExecutionFailed",
        "Interpreter",
        "InterpreterError",
        "InterpreterNotFoundError",
        "NotShareableError",
        "Queue",
        "QueueEmpty",
        "QueueFull",
        "create",
        "create_queue",
        "get_current",
        "get_main",
        "is_shareable",
        "list_all",
    ]

    _R = TypeVar("_R")
    _P = ParamSpec("_P")

    class ExecutionFailed(InterpreterError):
        """An unhandled exception happened during execution.

        This is raised from Interpreter.exec() and Interpreter.call().
        """

        excinfo: types.SimpleNamespace

        def __init__(self, excinfo: types.SimpleNamespace) -> None: ...

    def create() -> Interpreter:
        """Return a new (idle) Python interpreter."""

    def list_all() -> list[Interpreter]:
        """Return all existing interpreters."""

    def get_current() -> Interpreter:
        """Return the currently running interpreter."""

    def get_main() -> Interpreter:
        """Return the main interpreter."""

    class Interpreter:
        """A single Python interpreter.

        Attributes:

        "id" - the unique process-global ID number for the interpreter
        "whence" - indicates where the interpreter was created

        If the interpreter wasn't created by this module
        then any method that modifies the interpreter will fail,
        i.e. .close(), .prepare_main(), .exec(), and .call()
        """

        def __new__(cls, id: int, /, _whence: _Whence | None = None, _ownsref: bool | None = None) -> Self: ...
        def __reduce__(self) -> tuple[type[Self], int]: ...
        def __hash__(self) -> int: ...
        def __del__(self) -> None: ...
        @property
        def id(self) -> int: ...
        @property
        def whence(
            self,
        ) -> Literal["unknown", "runtime init", "legacy C-API", "C-API", "cross-interpreter C-API", "_interpreters module"]: ...
        def is_running(self) -> bool:
            """Return whether or not the identified interpreter is running."""

        def close(self) -> None:
            """Finalize and destroy the interpreter.

            Attempting to destroy the current interpreter results
            in an InterpreterError.
            """

        def prepare_main(
            self, ns: _SharedDict | None = None, /, **kwargs: Any
        ) -> None:  # kwargs has same value restrictions as _SharedDict
            """Bind the given values into the interpreter's __main__.

            The values must be shareable.
            """

        def exec(self, code: str | types.CodeType | Callable[[], object], /) -> None:
            """Run the given source code in the interpreter.

            This is essentially the same as calling the builtin "exec"
            with this interpreter, using the __dict__ of its __main__
            module as both globals and locals.

            There is no return value.

            If the code raises an unhandled exception then an ExecutionFailed
            exception is raised, which summarizes the unhandled exception.
            The actual exception is discarded because objects cannot be
            shared between interpreters.

            This blocks the current Python thread until done.  During
            that time, the previous interpreter is allowed to run
            in other threads.
            """

        def call(self, callable: Callable[_P, _R], /, *args: _P.args, **kwargs: _P.kwargs) -> _R:
            """Call the object in the interpreter with given args/kwargs.

            Nearly all callables, args, kwargs, and return values are
            supported.  All "shareable" objects are supported, as are
            "stateless" functions (meaning non-closures that do not use
            any globals).  This method will fall back to pickle.

            If the callable raises an exception then the error display
            (including full traceback) is sent back between the interpreters
            and an ExecutionFailed exception is raised, much like what
            happens with Interpreter.exec().
            """

        def call_in_thread(self, callable: Callable[_P, object], /, *args: _P.args, **kwargs: _P.kwargs) -> threading.Thread:
            """Return a new thread that calls the object in the interpreter.

            The return value and any raised exception are discarded.
            """
