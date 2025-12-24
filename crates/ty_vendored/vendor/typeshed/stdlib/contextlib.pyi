"""Utilities for with-statement contexts.  See PEP 343."""

import abc
import sys
from _typeshed import FileDescriptorOrPath, Unused
from abc import ABC, abstractmethod
from collections.abc import AsyncGenerator, AsyncIterator, Awaitable, Callable, Generator, Iterator
from types import TracebackType
from typing import Any, Generic, Protocol, TypeVar, overload, runtime_checkable, type_check_only
from typing_extensions import ParamSpec, Self, TypeAlias

__all__ = [
    "contextmanager",
    "closing",
    "AbstractContextManager",
    "ContextDecorator",
    "ExitStack",
    "redirect_stdout",
    "redirect_stderr",
    "suppress",
    "AbstractAsyncContextManager",
    "AsyncExitStack",
    "asynccontextmanager",
    "nullcontext",
]

if sys.version_info >= (3, 10):
    __all__ += ["aclosing"]

if sys.version_info >= (3, 11):
    __all__ += ["chdir"]

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_ExitT_co = TypeVar("_ExitT_co", covariant=True, bound=bool | None, default=bool | None)
_F = TypeVar("_F", bound=Callable[..., Any])
_G_co = TypeVar("_G_co", bound=Generator[Any, Any, Any] | AsyncGenerator[Any, Any], covariant=True)
_P = ParamSpec("_P")

_SendT_contra = TypeVar("_SendT_contra", contravariant=True, default=None)
_ReturnT_co = TypeVar("_ReturnT_co", covariant=True, default=None)

_ExitFunc: TypeAlias = Callable[[type[BaseException] | None, BaseException | None, TracebackType | None], bool | None]
_CM_EF = TypeVar("_CM_EF", bound=AbstractContextManager[Any, Any] | _ExitFunc)

# mypy and pyright object to this being both ABC and Protocol.
# At runtime it inherits from ABC and is not a Protocol, but it is on the
# allowlist for use as a Protocol.
@runtime_checkable
class AbstractContextManager(ABC, Protocol[_T_co, _ExitT_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    """An abstract base class for context managers."""

    __slots__ = ()
    def __enter__(self) -> _T_co:
        """Return `self` upon entering the runtime context."""

    @abstractmethod
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
    ) -> _ExitT_co:
        """Raise any exception triggered within the runtime context."""

# mypy and pyright object to this being both ABC and Protocol.
# At runtime it inherits from ABC and is not a Protocol, but it is on the
# allowlist for use as a Protocol.
@runtime_checkable
class AbstractAsyncContextManager(ABC, Protocol[_T_co, _ExitT_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    """An abstract base class for asynchronous context managers."""

    __slots__ = ()
    async def __aenter__(self) -> _T_co:
        """Return `self` upon entering the runtime context."""

    @abstractmethod
    async def __aexit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
    ) -> _ExitT_co:
        """Raise any exception triggered within the runtime context."""

class ContextDecorator:
    """A base class or mixin that enables context managers to work as decorators."""

    def _recreate_cm(self) -> Self:
        """Return a recreated instance of self.

        Allows an otherwise one-shot context manager like
        _GeneratorContextManager to support use as
        a decorator via implicit recreation.

        This is a private interface just for _GeneratorContextManager.
        See issue #11647 for details.
        """

    def __call__(self, func: _F) -> _F: ...

class _GeneratorContextManagerBase(Generic[_G_co]):
    """Shared functionality for @contextmanager and @asynccontextmanager."""

    # Ideally this would use ParamSpec, but that requires (*args, **kwargs), which this isn't. see #6676
    def __init__(self, func: Callable[..., _G_co], args: tuple[Any, ...], kwds: dict[str, Any]) -> None: ...
    gen: _G_co
    func: Callable[..., _G_co]
    args: tuple[Any, ...]
    kwds: dict[str, Any]

class _GeneratorContextManager(
    _GeneratorContextManagerBase[Generator[_T_co, _SendT_contra, _ReturnT_co]],
    AbstractContextManager[_T_co, bool | None],
    ContextDecorator,
):
    """Helper for @contextmanager decorator."""

    def __exit__(
        self, typ: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> bool | None: ...

def contextmanager(func: Callable[_P, Iterator[_T_co]]) -> Callable[_P, _GeneratorContextManager[_T_co]]:
    """@contextmanager decorator.

    Typical usage:

        @contextmanager
        def some_generator(<arguments>):
            <setup>
            try:
                yield <value>
            finally:
                <cleanup>

    This makes this:

        with some_generator(<arguments>) as <variable>:
            <body>

    equivalent to this:

        <setup>
        try:
            <variable> = <value>
            <body>
        finally:
            <cleanup>
    """

if sys.version_info >= (3, 10):
    _AF = TypeVar("_AF", bound=Callable[..., Awaitable[Any]])

    class AsyncContextDecorator:
        """A base class or mixin that enables async context managers to work as decorators."""

        def _recreate_cm(self) -> Self:
            """Return a recreated instance of self."""

        def __call__(self, func: _AF) -> _AF: ...

    class _AsyncGeneratorContextManager(
        _GeneratorContextManagerBase[AsyncGenerator[_T_co, _SendT_contra]],
        AbstractAsyncContextManager[_T_co, bool | None],
        AsyncContextDecorator,
    ):
        """Helper for @asynccontextmanager decorator."""

        async def __aexit__(
            self, typ: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
        ) -> bool | None: ...

else:
    class _AsyncGeneratorContextManager(
        _GeneratorContextManagerBase[AsyncGenerator[_T_co, _SendT_contra]], AbstractAsyncContextManager[_T_co, bool | None]
    ):
        """Helper for @asynccontextmanager decorator."""

        async def __aexit__(
            self, typ: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
        ) -> bool | None: ...

def asynccontextmanager(func: Callable[_P, AsyncIterator[_T_co]]) -> Callable[_P, _AsyncGeneratorContextManager[_T_co]]:
    """@asynccontextmanager decorator.

    Typical usage:

        @asynccontextmanager
        async def some_async_generator(<arguments>):
            <setup>
            try:
                yield <value>
            finally:
                <cleanup>

    This makes this:

        async with some_async_generator(<arguments>) as <variable>:
            <body>

    equivalent to this:

        <setup>
        try:
            <variable> = <value>
            <body>
        finally:
            <cleanup>
    """

@type_check_only
class _SupportsClose(Protocol):
    def close(self) -> object: ...

_SupportsCloseT = TypeVar("_SupportsCloseT", bound=_SupportsClose)

class closing(AbstractContextManager[_SupportsCloseT, None]):
    """Context to automatically close something at the end of a block.

    Code like this:

        with closing(<module>.open(<arguments>)) as f:
            <block>

    is equivalent to this:

        f = <module>.open(<arguments>)
        try:
            <block>
        finally:
            f.close()

    """

    def __init__(self, thing: _SupportsCloseT) -> None: ...
    def __exit__(self, *exc_info: Unused) -> None: ...

if sys.version_info >= (3, 10):
    @type_check_only
    class _SupportsAclose(Protocol):
        def aclose(self) -> Awaitable[object]: ...

    _SupportsAcloseT = TypeVar("_SupportsAcloseT", bound=_SupportsAclose)

    class aclosing(AbstractAsyncContextManager[_SupportsAcloseT, None]):
        """Async context manager for safely finalizing an asynchronously cleaned-up
        resource such as an async generator, calling its ``aclose()`` method.

        Code like this:

            async with aclosing(<module>.fetch(<arguments>)) as agen:
                <block>

        is equivalent to this:

            agen = <module>.fetch(<arguments>)
            try:
                <block>
            finally:
                await agen.aclose()

        """

        def __init__(self, thing: _SupportsAcloseT) -> None: ...
        async def __aexit__(self, *exc_info: Unused) -> None: ...

class suppress(AbstractContextManager[None, bool]):
    """Context manager to suppress specified exceptions

    After the exception is suppressed, execution proceeds with the next
    statement following the with statement.

         with suppress(FileNotFoundError):
             os.remove(somefile)
         # Execution still resumes here if the file was already removed
    """

    def __init__(self, *exceptions: type[BaseException]) -> None: ...
    def __exit__(
        self, exctype: type[BaseException] | None, excinst: BaseException | None, exctb: TracebackType | None
    ) -> bool: ...

# This is trying to describe what is needed for (most?) uses
# of `redirect_stdout` and `redirect_stderr`.
# https://github.com/python/typeshed/issues/14903
@type_check_only
class _SupportsRedirect(Protocol):
    def write(self, s: str, /) -> int: ...
    def flush(self) -> None: ...

_SupportsRedirectT = TypeVar("_SupportsRedirectT", bound=_SupportsRedirect | None)

class _RedirectStream(AbstractContextManager[_SupportsRedirectT, None]):
    def __init__(self, new_target: _SupportsRedirectT) -> None: ...
    def __exit__(
        self, exctype: type[BaseException] | None, excinst: BaseException | None, exctb: TracebackType | None
    ) -> None: ...

class redirect_stdout(_RedirectStream[_SupportsRedirectT]):
    """Context manager for temporarily redirecting stdout to another file.

    # How to send help() to stderr
    with redirect_stdout(sys.stderr):
        help(dir)

    # How to write help() to a file
    with open('help.txt', 'w') as f:
        with redirect_stdout(f):
            help(pow)
    """

class redirect_stderr(_RedirectStream[_SupportsRedirectT]):
    """Context manager for temporarily redirecting stderr to another file."""

class _BaseExitStack(Generic[_ExitT_co]):
    """A base class for ExitStack and AsyncExitStack."""

    def enter_context(self, cm: AbstractContextManager[_T, _ExitT_co]) -> _T:
        """Enters the supplied context manager.

        If successful, also pushes its __exit__ method as a callback and
        returns the result of the __enter__ method.
        """

    def push(self, exit: _CM_EF) -> _CM_EF:
        """Registers a callback with the standard __exit__ method signature.

        Can suppress exceptions the same way __exit__ method can.
        Also accepts any object with an __exit__ method (registering a call
        to the method instead of the object itself).
        """

    def callback(self, callback: Callable[_P, _T], /, *args: _P.args, **kwds: _P.kwargs) -> Callable[_P, _T]:
        """Registers an arbitrary callback and arguments.

        Cannot suppress exceptions.
        """

    def pop_all(self) -> Self:
        """Preserve the context stack by transferring it to a new instance."""

# In reality this is a subclass of `AbstractContextManager`;
# see #7961 for why we don't do that in the stub
class ExitStack(_BaseExitStack[_ExitT_co], metaclass=abc.ABCMeta):
    """Context manager for dynamic management of a stack of exit callbacks.

    For example:
        with ExitStack() as stack:
            files = [stack.enter_context(open(fname)) for fname in filenames]
            # All opened files will automatically be closed at the end of
            # the with statement, even if attempts to open files later
            # in the list raise an exception.
    """

    def close(self) -> None:
        """Immediately unwind the context stack."""

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
    ) -> _ExitT_co: ...

_ExitCoroFunc: TypeAlias = Callable[
    [type[BaseException] | None, BaseException | None, TracebackType | None], Awaitable[bool | None]
]
_ACM_EF = TypeVar("_ACM_EF", bound=AbstractAsyncContextManager[Any, Any] | _ExitCoroFunc)

# In reality this is a subclass of `AbstractAsyncContextManager`;
# see #7961 for why we don't do that in the stub
class AsyncExitStack(_BaseExitStack[_ExitT_co], metaclass=abc.ABCMeta):
    """Async context manager for dynamic management of a stack of exit
    callbacks.

    For example:
        async with AsyncExitStack() as stack:
            connections = [await stack.enter_async_context(get_connection())
                for i in range(5)]
            # All opened connections will automatically be released at the
            # end of the async with statement, even if attempts to open a
            # connection later in the list raise an exception.
    """

    async def enter_async_context(self, cm: AbstractAsyncContextManager[_T, _ExitT_co]) -> _T:
        """Enters the supplied async context manager.

        If successful, also pushes its __aexit__ method as a callback and
        returns the result of the __aenter__ method.
        """

    def push_async_exit(self, exit: _ACM_EF) -> _ACM_EF:
        """Registers a coroutine function with the standard __aexit__ method
        signature.

        Can suppress exceptions the same way __aexit__ method can.
        Also accepts any object with an __aexit__ method (registering a call
        to the method instead of the object itself).
        """

    def push_async_callback(
        self, callback: Callable[_P, Awaitable[_T]], /, *args: _P.args, **kwds: _P.kwargs
    ) -> Callable[_P, Awaitable[_T]]:
        """Registers an arbitrary coroutine function and arguments.

        Cannot suppress exceptions.
        """

    async def aclose(self) -> None:
        """Immediately unwind the context stack."""

    async def __aenter__(self) -> Self: ...
    async def __aexit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
    ) -> _ExitT_co: ...

if sys.version_info >= (3, 10):
    class nullcontext(AbstractContextManager[_T, None], AbstractAsyncContextManager[_T, None]):
        """Context manager that does no additional processing.

        Used as a stand-in for a normal context manager, when a particular
        block of code is only sometimes used with a normal context manager:

        cm = optional_cm if condition else nullcontext()
        with cm:
            # Perform operation, using optional_cm if condition is True
        """

        enter_result: _T
        @overload
        def __init__(self: nullcontext[None], enter_result: None = None) -> None: ...
        @overload
        def __init__(self: nullcontext[_T], enter_result: _T) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        def __enter__(self) -> _T: ...
        def __exit__(self, *exctype: Unused) -> None: ...
        async def __aenter__(self) -> _T: ...
        async def __aexit__(self, *exctype: Unused) -> None: ...

else:
    class nullcontext(AbstractContextManager[_T, None]):
        """Context manager that does no additional processing.

        Used as a stand-in for a normal context manager, when a particular
        block of code is only sometimes used with a normal context manager:

        cm = optional_cm if condition else nullcontext()
        with cm:
            # Perform operation, using optional_cm if condition is True
        """

        enter_result: _T
        @overload
        def __init__(self: nullcontext[None], enter_result: None = None) -> None: ...
        @overload
        def __init__(self: nullcontext[_T], enter_result: _T) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        def __enter__(self) -> _T: ...
        def __exit__(self, *exctype: Unused) -> None: ...

if sys.version_info >= (3, 11):
    _T_fd_or_any_path = TypeVar("_T_fd_or_any_path", bound=FileDescriptorOrPath)

    class chdir(AbstractContextManager[None, None], Generic[_T_fd_or_any_path]):
        """Non thread-safe context manager to change the current working directory."""

        path: _T_fd_or_any_path
        def __init__(self, path: _T_fd_or_any_path) -> None: ...
        def __enter__(self) -> None: ...
        def __exit__(self, *excinfo: Unused) -> None: ...
