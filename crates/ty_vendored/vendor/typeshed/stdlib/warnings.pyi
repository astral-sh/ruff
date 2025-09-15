"""Python part of the warnings subsystem."""

import re
import sys
from _warnings import warn as warn, warn_explicit as warn_explicit
from collections.abc import Sequence
from types import ModuleType, TracebackType
from typing import Any, Generic, Literal, TextIO, overload
from typing_extensions import LiteralString, TypeAlias, TypeVar

__all__ = [
    "warn",
    "warn_explicit",
    "showwarning",
    "formatwarning",
    "filterwarnings",
    "simplefilter",
    "resetwarnings",
    "catch_warnings",
]

if sys.version_info >= (3, 13):
    __all__ += ["deprecated"]

_T = TypeVar("_T")
_W_co = TypeVar("_W_co", bound=list[WarningMessage] | None, default=list[WarningMessage] | None, covariant=True)

if sys.version_info >= (3, 14):
    _ActionKind: TypeAlias = Literal["default", "error", "ignore", "always", "module", "once"]
else:
    _ActionKind: TypeAlias = Literal["default", "error", "ignore", "always", "all", "module", "once"]
filters: Sequence[tuple[str, re.Pattern[str] | None, type[Warning], re.Pattern[str] | None, int]]  # undocumented, do not mutate

def showwarning(
    message: Warning | str,
    category: type[Warning],
    filename: str,
    lineno: int,
    file: TextIO | None = None,
    line: str | None = None,
) -> None:
    """Hook to write a warning to a file; replace if you like."""

def formatwarning(message: Warning | str, category: type[Warning], filename: str, lineno: int, line: str | None = None) -> str:
    """Function to format a warning the standard way."""

def filterwarnings(
    action: _ActionKind, message: str = "", category: type[Warning] = ..., module: str = "", lineno: int = 0, append: bool = False
) -> None:
    """Insert an entry into the list of warnings filters (at the front).

    'action' -- one of "error", "ignore", "always", "all", "default", "module",
                or "once"
    'message' -- a regex that the warning message must match
    'category' -- a class that the warning must be a subclass of
    'module' -- a regex that the module name must match
    'lineno' -- an integer line number, 0 matches all warnings
    'append' -- if true, append to the list of filters
    """

def simplefilter(action: _ActionKind, category: type[Warning] = ..., lineno: int = 0, append: bool = False) -> None:
    """Insert a simple entry into the list of warnings filters (at the front).

    A simple filter matches all modules and messages.
    'action' -- one of "error", "ignore", "always", "all", "default", "module",
                or "once"
    'category' -- a class that the warning must be a subclass of
    'lineno' -- an integer line number, 0 matches all warnings
    'append' -- if true, append to the list of filters
    """

def resetwarnings() -> None:
    """Clear the list of warning filters, so that no filters are active."""

class _OptionError(Exception):
    """Exception used by option processing helpers."""

class WarningMessage:
    message: Warning | str
    category: type[Warning]
    filename: str
    lineno: int
    file: TextIO | None
    line: str | None
    source: Any | None
    def __init__(
        self,
        message: Warning | str,
        category: type[Warning],
        filename: str,
        lineno: int,
        file: TextIO | None = None,
        line: str | None = None,
        source: Any | None = None,
    ) -> None: ...

class catch_warnings(Generic[_W_co]):
    """A context manager that copies and restores the warnings filter upon
    exiting the context.

    The 'record' argument specifies whether warnings should be captured by a
    custom implementation of warnings.showwarning() and be appended to a list
    returned by the context manager. Otherwise None is returned by the context
    manager. The objects appended to the list are arguments whose attributes
    mirror the arguments to showwarning().

    The 'module' argument is to specify an alternative module to the module
    named 'warnings' and imported under that name. This argument is only useful
    when testing the warnings module itself.

    If the 'action' argument is not None, the remaining arguments are passed
    to warnings.simplefilter() as if it were called immediately on entering the
    context.
    """

    if sys.version_info >= (3, 11):
        @overload
        def __init__(
            self: catch_warnings[None],
            *,
            record: Literal[False] = False,
            module: ModuleType | None = None,
            action: _ActionKind | None = None,
            category: type[Warning] = ...,
            lineno: int = 0,
            append: bool = False,
        ) -> None:
            """Specify whether to record warnings and if an alternative module
            should be used other than sys.modules['warnings'].

            """

        @overload
        def __init__(
            self: catch_warnings[list[WarningMessage]],
            *,
            record: Literal[True],
            module: ModuleType | None = None,
            action: _ActionKind | None = None,
            category: type[Warning] = ...,
            lineno: int = 0,
            append: bool = False,
        ) -> None: ...
        @overload
        def __init__(
            self,
            *,
            record: bool,
            module: ModuleType | None = None,
            action: _ActionKind | None = None,
            category: type[Warning] = ...,
            lineno: int = 0,
            append: bool = False,
        ) -> None: ...
    else:
        @overload
        def __init__(self: catch_warnings[None], *, record: Literal[False] = False, module: ModuleType | None = None) -> None:
            """Specify whether to record warnings and if an alternative module
            should be used other than sys.modules['warnings'].

            For compatibility with Python 3.0, please consider all arguments to be
            keyword-only.

            """

        @overload
        def __init__(
            self: catch_warnings[list[WarningMessage]], *, record: Literal[True], module: ModuleType | None = None
        ) -> None: ...
        @overload
        def __init__(self, *, record: bool, module: ModuleType | None = None) -> None: ...

    def __enter__(self) -> _W_co: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...

if sys.version_info >= (3, 13):
    class deprecated:
        """Indicate that a class, function or overload is deprecated.

        When this decorator is applied to an object, the type checker
        will generate a diagnostic on usage of the deprecated object.

        Usage:

            @deprecated("Use B instead")
            class A:
                pass

            @deprecated("Use g instead")
            def f():
                pass

            @overload
            @deprecated("int support is deprecated")
            def g(x: int) -> int: ...
            @overload
            def g(x: str) -> int: ...

        The warning specified by *category* will be emitted at runtime
        on use of deprecated objects. For functions, that happens on calls;
        for classes, on instantiation and on creation of subclasses.
        If the *category* is ``None``, no warning is emitted at runtime.
        The *stacklevel* determines where the
        warning is emitted. If it is ``1`` (the default), the warning
        is emitted at the direct caller of the deprecated object; if it
        is higher, it is emitted further up the stack.
        Static type checker behavior is not affected by the *category*
        and *stacklevel* arguments.

        The deprecation message passed to the decorator is saved in the
        ``__deprecated__`` attribute on the decorated object.
        If applied to an overload, the decorator
        must be after the ``@overload`` decorator for the attribute to
        exist on the overload as returned by ``get_overloads()``.

        See PEP 702 for details.

        """

        message: LiteralString
        category: type[Warning] | None
        stacklevel: int
        def __init__(self, message: LiteralString, /, *, category: type[Warning] | None = ..., stacklevel: int = 1) -> None: ...
        def __call__(self, arg: _T, /) -> _T: ...
