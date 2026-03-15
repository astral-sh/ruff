"""Selector event loop for Unix with signal handling."""

import sys
import types
from _typeshed import StrPath
from abc import ABCMeta, abstractmethod
from collections.abc import Callable
from socket import socket
from typing import Literal
from typing_extensions import Self, TypeVarTuple, Unpack, deprecated

from . import events
from .base_events import Server, _ProtocolFactory, _SSLContext
from .selector_events import BaseSelectorEventLoop

_Ts = TypeVarTuple("_Ts")

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.platform != "win32":
    if sys.version_info >= (3, 14):
        __all__ = ("SelectorEventLoop", "EventLoop")
    elif sys.version_info >= (3, 13):
        # Adds EventLoop
        __all__ = (
            "SelectorEventLoop",
            "AbstractChildWatcher",
            "SafeChildWatcher",
            "FastChildWatcher",
            "PidfdChildWatcher",
            "MultiLoopChildWatcher",
            "ThreadedChildWatcher",
            "DefaultEventLoopPolicy",
            "EventLoop",
        )
    else:
        # adds PidfdChildWatcher
        __all__ = (
            "SelectorEventLoop",
            "AbstractChildWatcher",
            "SafeChildWatcher",
            "FastChildWatcher",
            "PidfdChildWatcher",
            "MultiLoopChildWatcher",
            "ThreadedChildWatcher",
            "DefaultEventLoopPolicy",
        )

# This is also technically not available on Win,
# but other parts of typeshed need this definition.
# So, it is special cased.
if sys.version_info < (3, 14):
    if sys.version_info >= (3, 12):
        @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
        class AbstractChildWatcher:
            """Abstract base class for monitoring child processes.

            Objects derived from this class monitor a collection of subprocesses and
            report their termination or interruption by a signal.

            New callbacks are registered with .add_child_handler(). Starting a new
            process must be done within a 'with' block to allow the watcher to suspend
            its activity until the new process if fully registered (this is needed to
            prevent a race condition in some implementations).

            Example:
                with watcher:
                    proc = subprocess.Popen("sleep 1")
                    watcher.add_child_handler(proc.pid, callback)

            Notes:
                Implementations of this class must be thread-safe.

                Since child watcher objects may catch the SIGCHLD signal and call
                waitpid(-1), there should be only one active object per process.
            """

            @abstractmethod
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None:
                """Register a new child handler.

                Arrange for callback(pid, returncode, *args) to be called when
                process 'pid' terminates. Specifying another callback for the same
                process replaces the previous handler.

                Note: callback() must be thread-safe.
                """

            @abstractmethod
            def remove_child_handler(self, pid: int) -> bool:
                """Removes the handler for process 'pid'.

                The function returns True if the handler was successfully removed,
                False if there was nothing to remove.
                """

            @abstractmethod
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None:
                """Attach the watcher to an event loop.

                If the watcher was previously attached to an event loop, then it is
                first detached before attaching to the new loop.

                Note: loop may be None.
                """

            @abstractmethod
            def close(self) -> None:
                """Close the watcher.

                This must be called to make sure that any underlying resource is freed.
                """

            @abstractmethod
            def __enter__(self) -> Self:
                """Enter the watcher's context and allow starting new processes

                This function must return self
                """

            @abstractmethod
            def __exit__(
                self, typ: type[BaseException] | None, exc: BaseException | None, tb: types.TracebackType | None
            ) -> None:
                """Exit the watcher's context"""

            @abstractmethod
            def is_active(self) -> bool:
                """Return ``True`` if the watcher is active and is used by the event loop.

                Return True if the watcher is installed and ready to handle process exit
                notifications.

                """

    else:
        class AbstractChildWatcher:
            """Abstract base class for monitoring child processes.

            Objects derived from this class monitor a collection of subprocesses and
            report their termination or interruption by a signal.

            New callbacks are registered with .add_child_handler(). Starting a new
            process must be done within a 'with' block to allow the watcher to suspend
            its activity until the new process if fully registered (this is needed to
            prevent a race condition in some implementations).

            Example:
                with watcher:
                    proc = subprocess.Popen("sleep 1")
                    watcher.add_child_handler(proc.pid, callback)

            Notes:
                Implementations of this class must be thread-safe.

                Since child watcher objects may catch the SIGCHLD signal and call
                waitpid(-1), there should be only one active object per process.
            """

            @abstractmethod
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None:
                """Register a new child handler.

                Arrange for callback(pid, returncode, *args) to be called when
                process 'pid' terminates. Specifying another callback for the same
                process replaces the previous handler.

                Note: callback() must be thread-safe.
                """

            @abstractmethod
            def remove_child_handler(self, pid: int) -> bool:
                """Removes the handler for process 'pid'.

                The function returns True if the handler was successfully removed,
                False if there was nothing to remove.
                """

            @abstractmethod
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None:
                """Attach the watcher to an event loop.

                If the watcher was previously attached to an event loop, then it is
                first detached before attaching to the new loop.

                Note: loop may be None.
                """

            @abstractmethod
            def close(self) -> None:
                """Close the watcher.

                This must be called to make sure that any underlying resource is freed.
                """

            @abstractmethod
            def __enter__(self) -> Self:
                """Enter the watcher's context and allow starting new processes

                This function must return self
                """

            @abstractmethod
            def __exit__(
                self, typ: type[BaseException] | None, exc: BaseException | None, tb: types.TracebackType | None
            ) -> None:
                """Exit the watcher's context"""

            @abstractmethod
            def is_active(self) -> bool:
                """Return ``True`` if the watcher is active and is used by the event loop.

                Return True if the watcher is installed and ready to handle process exit
                notifications.

                """

if sys.platform != "win32":
    if sys.version_info < (3, 14):
        if sys.version_info >= (3, 12):
            # Doesn't actually have ABCMeta metaclass at runtime, but mypy complains if we don't have it in the stub.
            # See discussion in #7412
            class BaseChildWatcher(AbstractChildWatcher, metaclass=ABCMeta):
                def close(self) -> None: ...
                def is_active(self) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class SafeChildWatcher(BaseChildWatcher):
                """'Safe' child watcher implementation.

                This implementation avoids disrupting other code spawning processes by
                polling explicitly each process in the SIGCHLD handler instead of calling
                os.waitpid(-1).

                This is a safe solution but it has a significant overhead when handling a
                big number of children (O(n) each time SIGCHLD is raised)
                """

                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class FastChildWatcher(BaseChildWatcher):
                """'Fast' child watcher implementation.

                This implementation reaps every terminated processes by calling
                os.waitpid(-1) directly, possibly breaking other code spawning processes
                and waiting for their termination.

                There is no noticeable overhead when handling a big number of children
                (O(1) each time a child terminates).
                """

                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

        else:
            # Doesn't actually have ABCMeta metaclass at runtime, but mypy complains if we don't have it in the stub.
            # See discussion in #7412
            class BaseChildWatcher(AbstractChildWatcher, metaclass=ABCMeta):
                def close(self) -> None: ...
                def is_active(self) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

            class SafeChildWatcher(BaseChildWatcher):
                """'Safe' child watcher implementation.

                This implementation avoids disrupting other code spawning processes by
                polling explicitly each process in the SIGCHLD handler instead of calling
                os.waitpid(-1).

                This is a safe solution but it has a significant overhead when handling a
                big number of children (O(n) each time SIGCHLD is raised)
                """

                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

            class FastChildWatcher(BaseChildWatcher):
                """'Fast' child watcher implementation.

                This implementation reaps every terminated processes by calling
                os.waitpid(-1) directly, possibly breaking other code spawning processes
                and waiting for their termination.

                There is no noticeable overhead when handling a big number of children
                (O(1) each time a child terminates).
                """

                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

    class _UnixSelectorEventLoop(BaseSelectorEventLoop):
        """Unix event loop.

        Adds signal handling and UNIX Domain Socket support to SelectorEventLoop.
        """

        if sys.version_info >= (3, 13):
            async def create_unix_server(
                self,
                protocol_factory: _ProtocolFactory,
                path: StrPath | None = None,
                *,
                sock: socket | None = None,
                backlog: int = 100,
                ssl: _SSLContext = None,
                ssl_handshake_timeout: float | None = None,
                ssl_shutdown_timeout: float | None = None,
                start_serving: bool = True,
                cleanup_socket: bool = True,
            ) -> Server: ...

    if sys.version_info >= (3, 14):
        class _UnixDefaultEventLoopPolicy(events._BaseDefaultEventLoopPolicy):
            """UNIX event loop policy"""

    else:
        class _UnixDefaultEventLoopPolicy(events.BaseDefaultEventLoopPolicy):
            """UNIX event loop policy with a watcher for child processes."""

            if sys.version_info >= (3, 12):
                @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
                def get_child_watcher(self) -> AbstractChildWatcher:
                    """Get the watcher for child processes.

                    If not yet set, a ThreadedChildWatcher object is automatically created.
                    """

                @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
                def set_child_watcher(self, watcher: AbstractChildWatcher | None) -> None:
                    """Set the watcher for child processes."""
            else:
                def get_child_watcher(self) -> AbstractChildWatcher:
                    """Get the watcher for child processes.

                    If not yet set, a ThreadedChildWatcher object is automatically created.
                    """

                def set_child_watcher(self, watcher: AbstractChildWatcher | None) -> None:
                    """Set the watcher for child processes."""

    SelectorEventLoop = _UnixSelectorEventLoop

    if sys.version_info >= (3, 14):
        _DefaultEventLoopPolicy = _UnixDefaultEventLoopPolicy
    else:
        DefaultEventLoopPolicy = _UnixDefaultEventLoopPolicy

    if sys.version_info >= (3, 13):
        EventLoop = SelectorEventLoop

    if sys.version_info < (3, 14):
        if sys.version_info >= (3, 12):
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class MultiLoopChildWatcher(AbstractChildWatcher):
                """A watcher that doesn't require running loop in the main thread.

                This implementation registers a SIGCHLD signal handler on
                instantiation (which may conflict with other code that
                install own handler for this signal).

                The solution is safe but it has a significant overhead when
                handling a big number of processes (*O(n)* each time a
                SIGCHLD is received).
                """

                def is_active(self) -> bool: ...
                def close(self) -> None: ...
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

        else:
            class MultiLoopChildWatcher(AbstractChildWatcher):
                """A watcher that doesn't require running loop in the main thread.

                This implementation registers a SIGCHLD signal handler on
                instantiation (which may conflict with other code that
                install own handler for this signal).

                The solution is safe but it has a significant overhead when
                handling a big number of processes (*O(n)* each time a
                SIGCHLD is received).
                """

                def is_active(self) -> bool: ...
                def close(self) -> None: ...
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

    if sys.version_info < (3, 14):
        class ThreadedChildWatcher(AbstractChildWatcher):
            """Threaded child watcher implementation.

            The watcher uses a thread per process
            for waiting for the process finish.

            It doesn't require subscription on POSIX signal
            but a thread creation is not free.

            The watcher has O(1) complexity, its performance doesn't depend
            on amount of spawn processes.
            """

            def is_active(self) -> Literal[True]: ...
            def close(self) -> None: ...
            def __enter__(self) -> Self: ...
            def __exit__(
                self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
            ) -> None: ...
            def __del__(self) -> None: ...
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            def remove_child_handler(self, pid: int) -> bool: ...
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

        class PidfdChildWatcher(AbstractChildWatcher):
            """Child watcher implementation using Linux's pid file descriptors.

            This child watcher polls process file descriptors (pidfds) to await child
            process termination. In some respects, PidfdChildWatcher is a "Goldilocks"
            child watcher implementation. It doesn't require signals or threads, doesn't
            interfere with any processes launched outside the event loop, and scales
            linearly with the number of subprocesses launched by the event loop. The
            main disadvantage is that pidfds are specific to Linux, and only work on
            recent (5.3+) kernels.
            """

            def __enter__(self) -> Self: ...
            def __exit__(
                self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
            ) -> None: ...
            def is_active(self) -> bool: ...
            def close(self) -> None: ...
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            def remove_child_handler(self, pid: int) -> bool: ...
