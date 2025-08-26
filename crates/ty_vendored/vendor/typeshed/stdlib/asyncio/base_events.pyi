"""Base implementation of event loop.

The event loop can be broken up into a multiplexer (the part
responsible for notifying us of I/O events) and the event loop proper,
which wraps a multiplexer with functionality for scheduling callbacks,
immediately or at a given time in the future.

Whenever a public API takes a callback, subsequent positional
arguments will be passed to the callback if/when it is called.  This
avoids the proliferation of trivial lambdas implementing closures.
Keyword arguments for the callback are not supported; this is a
conscious design decision, leaving the door open for keyword arguments
to modify the meaning of the API call itself.
"""

import ssl
import sys
from _typeshed import FileDescriptorLike, ReadableBuffer, WriteableBuffer
from asyncio import _AwaitableLike, _CoroutineLike
from asyncio.events import AbstractEventLoop, AbstractServer, Handle, TimerHandle, _TaskFactory
from asyncio.futures import Future
from asyncio.protocols import BaseProtocol
from asyncio.tasks import Task
from asyncio.transports import BaseTransport, DatagramTransport, ReadTransport, SubprocessTransport, Transport, WriteTransport
from collections.abc import Callable, Iterable, Sequence
from concurrent.futures import Executor, ThreadPoolExecutor
from contextvars import Context
from socket import AddressFamily, AddressInfo, SocketKind, _Address, _RetAddress, socket
from typing import IO, Any, Literal, TypeVar, overload
from typing_extensions import TypeAlias, TypeVarTuple, Unpack

# Keep asyncio.__all__ updated with any changes to __all__ here
__all__ = ("BaseEventLoop", "Server")

_T = TypeVar("_T")
_Ts = TypeVarTuple("_Ts")
_ProtocolT = TypeVar("_ProtocolT", bound=BaseProtocol)
_Context: TypeAlias = dict[str, Any]
_ExceptionHandler: TypeAlias = Callable[[AbstractEventLoop, _Context], object]
_ProtocolFactory: TypeAlias = Callable[[], BaseProtocol]
_SSLContext: TypeAlias = bool | None | ssl.SSLContext

class Server(AbstractServer):
    if sys.version_info >= (3, 11):
        def __init__(
            self,
            loop: AbstractEventLoop,
            sockets: Iterable[socket],
            protocol_factory: _ProtocolFactory,
            ssl_context: _SSLContext,
            backlog: int,
            ssl_handshake_timeout: float | None,
            ssl_shutdown_timeout: float | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            loop: AbstractEventLoop,
            sockets: Iterable[socket],
            protocol_factory: _ProtocolFactory,
            ssl_context: _SSLContext,
            backlog: int,
            ssl_handshake_timeout: float | None,
        ) -> None: ...

    if sys.version_info >= (3, 13):
        def close_clients(self) -> None: ...
        def abort_clients(self) -> None: ...

    def get_loop(self) -> AbstractEventLoop: ...
    def is_serving(self) -> bool: ...
    async def start_serving(self) -> None: ...
    async def serve_forever(self) -> None: ...
    @property
    def sockets(self) -> tuple[socket, ...]: ...
    def close(self) -> None: ...
    async def wait_closed(self) -> None:
        """Wait until server is closed and all connections are dropped.

        - If the server is not closed, wait.
        - If it is closed, but there are still active connections, wait.

        Anyone waiting here will be unblocked once both conditions
        (server is closed and all connections have been dropped)
        have become true, in either order.

        Historical note: In 3.11 and before, this was broken, returning
        immediately if the server was already closed, even if there
        were still active connections. An attempted fix in 3.12.0 was
        still broken, returning immediately if the server was still
        open and there were no active connections. Hopefully in 3.12.1
        we have it right.
        """

class BaseEventLoop(AbstractEventLoop):
    def run_forever(self) -> None:
        """Run until stop() is called."""

    def run_until_complete(self, future: _AwaitableLike[_T]) -> _T:
        """Run until the Future is done.

        If the argument is a coroutine, it is wrapped in a Task.

        WARNING: It would be disastrous to call run_until_complete()
        with the same coroutine twice -- it would wrap it in two
        different Tasks and that can't be good.

        Return the Future's result, or raise its exception.
        """

    def stop(self) -> None:
        """Stop running the event loop.

        Every callback already scheduled will still run.  This simply informs
        run_forever to stop looping after a complete iteration.
        """

    def is_running(self) -> bool:
        """Returns True if the event loop is running."""

    def is_closed(self) -> bool:
        """Returns True if the event loop was closed."""

    def close(self) -> None:
        """Close the event loop.

        This clears the queues and shuts down the executor,
        but does not wait for the executor to finish.

        The event loop must not be running.
        """

    async def shutdown_asyncgens(self) -> None:
        """Shutdown all active asynchronous generators."""
    # Methods scheduling callbacks.  All these return Handles.
    def call_soon(self, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None) -> Handle:
        """Arrange for a callback to be called as soon as possible.

        This operates as a FIFO queue: callbacks are called in the
        order in which they are registered.  Each callback will be
        called exactly once.

        Any positional arguments after the callback will be passed to
        the callback when it is called.
        """

    def call_later(
        self, delay: float, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> TimerHandle:
        """Arrange for a callback to be called at a given time.

        Return a Handle: an opaque object with a cancel() method that
        can be used to cancel the call.

        The delay can be an int or float, expressed in seconds.  It is
        always relative to the current time.

        Each callback will be called exactly once.  If two callbacks
        are scheduled for exactly the same time, it is undefined which
        will be called first.

        Any positional arguments after the callback will be passed to
        the callback when it is called.
        """

    def call_at(
        self, when: float, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> TimerHandle:
        """Like call_later(), but uses an absolute time.

        Absolute time corresponds to the event loop's time() method.
        """

    def time(self) -> float:
        """Return the time according to the event loop's clock.

        This is a float expressed in seconds since an epoch, but the
        epoch, precision, accuracy and drift are unspecified and may
        differ per event loop.
        """
    # Future methods
    def create_future(self) -> Future[Any]:
        """Create a Future object attached to the loop."""
    # Tasks methods
    if sys.version_info >= (3, 11):
        def create_task(self, coro: _CoroutineLike[_T], *, name: object = None, context: Context | None = None) -> Task[_T]:
            """Schedule or begin executing a coroutine object.

            Return a task object.
            """
    else:
        def create_task(self, coro: _CoroutineLike[_T], *, name: object = None) -> Task[_T]:
            """Schedule a coroutine object.

            Return a task object.
            """

    def set_task_factory(self, factory: _TaskFactory | None) -> None:
        """Set a task factory that will be used by loop.create_task().

        If factory is None the default task factory will be set.

        If factory is a callable, it should have a signature matching
        '(loop, coro, **kwargs)', where 'loop' will be a reference to the active
        event loop, 'coro' will be a coroutine object, and **kwargs will be
        arbitrary keyword arguments that should be passed on to Task.
        The callable must return a Task.
        """

    def get_task_factory(self) -> _TaskFactory | None:
        """Return a task factory, or None if the default one is in use."""
    # Methods for interacting with threads
    def call_soon_threadsafe(
        self, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> Handle:
        """Like call_soon(), but thread-safe."""

    def run_in_executor(self, executor: Executor | None, func: Callable[[Unpack[_Ts]], _T], *args: Unpack[_Ts]) -> Future[_T]: ...
    def set_default_executor(self, executor: ThreadPoolExecutor) -> None: ...  # type: ignore[override]
    # Network I/O methods returning Futures.
    async def getaddrinfo(
        self,
        host: bytes | str | None,
        port: bytes | str | int | None,
        *,
        family: int = 0,
        type: int = 0,
        proto: int = 0,
        flags: int = 0,
    ) -> list[tuple[AddressFamily, SocketKind, int, str, tuple[str, int] | tuple[str, int, int, int]]]: ...
    async def getnameinfo(self, sockaddr: tuple[str, int] | tuple[str, int, int, int], flags: int = 0) -> tuple[str, str]: ...
    if sys.version_info >= (3, 12):
        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: str = ...,
            port: int = ...,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: None = None,
            local_addr: tuple[str, int] | None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
            all_errors: bool = False,
        ) -> tuple[Transport, _ProtocolT]:
            """Connect to a TCP server.

            Create a streaming transport connection to a given internet host and
            port: socket family AF_INET or socket.AF_INET6 depending on host (or
            family if specified), socket type SOCK_STREAM. protocol_factory must be
            a callable returning a protocol instance.

            This method is a coroutine which will try to establish the connection
            in the background.  When successful, the coroutine returns a
            (transport, protocol) pair.
            """

        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: None = None,
            port: None = None,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: socket,
            local_addr: None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
            all_errors: bool = False,
        ) -> tuple[Transport, _ProtocolT]: ...
    elif sys.version_info >= (3, 11):
        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: str = ...,
            port: int = ...,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: None = None,
            local_addr: tuple[str, int] | None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
        ) -> tuple[Transport, _ProtocolT]:
            """Connect to a TCP server.

            Create a streaming transport connection to a given internet host and
            port: socket family AF_INET or socket.AF_INET6 depending on host (or
            family if specified), socket type SOCK_STREAM. protocol_factory must be
            a callable returning a protocol instance.

            This method is a coroutine which will try to establish the connection
            in the background.  When successful, the coroutine returns a
            (transport, protocol) pair.
            """

        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: None = None,
            port: None = None,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: socket,
            local_addr: None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
        ) -> tuple[Transport, _ProtocolT]: ...
    else:
        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: str = ...,
            port: int = ...,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: None = None,
            local_addr: tuple[str, int] | None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
        ) -> tuple[Transport, _ProtocolT]:
            """Connect to a TCP server.

            Create a streaming transport connection to a given internet host and
            port: socket family AF_INET or socket.AF_INET6 depending on host (or
            family if specified), socket type SOCK_STREAM. protocol_factory must be
            a callable returning a protocol instance.

            This method is a coroutine which will try to establish the connection
            in the background.  When successful, the coroutine returns a
            (transport, protocol) pair.
            """

        @overload
        async def create_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            host: None = None,
            port: None = None,
            *,
            ssl: _SSLContext = None,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            sock: socket,
            local_addr: None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            happy_eyeballs_delay: float | None = None,
            interleave: int | None = None,
        ) -> tuple[Transport, _ProtocolT]: ...

    if sys.version_info >= (3, 13):
        # 3.13 added `keep_alive`.
        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: str | Sequence[str] | None = None,
            port: int = ...,
            *,
            family: int = 0,
            flags: int = 1,
            sock: None = None,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            keep_alive: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server:
            """Create a TCP server.

            The host parameter can be a string, in that case the TCP server is
            bound to host and port.

            The host parameter can also be a sequence of strings and in that case
            the TCP server is bound to all hosts of the sequence. If a host
            appears multiple times (possibly indirectly e.g. when hostnames
            resolve to the same IP address), the server is only bound once to that
            host.

            Return a Server object which can be used to stop the service.

            This method is a coroutine.
            """

        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: None = None,
            port: None = None,
            *,
            family: int = 0,
            flags: int = 1,
            sock: socket = ...,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            keep_alive: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server: ...
    elif sys.version_info >= (3, 11):
        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: str | Sequence[str] | None = None,
            port: int = ...,
            *,
            family: int = AddressFamily.AF_UNSPEC,
            flags: int = AddressInfo.AI_PASSIVE,
            sock: None = None,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server:
            """Create a TCP server.

            The host parameter can be a string, in that case the TCP server is
            bound to host and port.

            The host parameter can also be a sequence of strings and in that case
            the TCP server is bound to all hosts of the sequence. If a host
            appears multiple times (possibly indirectly e.g. when hostnames
            resolve to the same IP address), the server is only bound once to that
            host.

            Return a Server object which can be used to stop the service.

            This method is a coroutine.
            """

        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: None = None,
            port: None = None,
            *,
            family: int = AddressFamily.AF_UNSPEC,
            flags: int = AddressInfo.AI_PASSIVE,
            sock: socket = ...,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server: ...
    else:
        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: str | Sequence[str] | None = None,
            port: int = ...,
            *,
            family: int = AddressFamily.AF_UNSPEC,
            flags: int = AddressInfo.AI_PASSIVE,
            sock: None = None,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server:
            """Create a TCP server.

            The host parameter can be a string, in that case the TCP server is
            bound to host and port.

            The host parameter can also be a sequence of strings and in that case
            the TCP server is bound to all hosts of the sequence. If a host
            appears multiple times (possibly indirectly e.g. when hostnames
            resolve to the same IP address), the server is only bound once to that
            host.

            Return a Server object which can be used to stop the service.

            This method is a coroutine.
            """

        @overload
        async def create_server(
            self,
            protocol_factory: _ProtocolFactory,
            host: None = None,
            port: None = None,
            *,
            family: int = AddressFamily.AF_UNSPEC,
            flags: int = AddressInfo.AI_PASSIVE,
            sock: socket = ...,
            backlog: int = 100,
            ssl: _SSLContext = None,
            reuse_address: bool | None = None,
            reuse_port: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server: ...

    if sys.version_info >= (3, 11):
        async def start_tls(
            self,
            transport: BaseTransport,
            protocol: BaseProtocol,
            sslcontext: ssl.SSLContext,
            *,
            server_side: bool = False,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> Transport | None:
            """Upgrade transport to TLS.

            Return a new transport that *protocol* should start using
            immediately.
            """

        async def connect_accepted_socket(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            sock: socket,
            *,
            ssl: _SSLContext = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> tuple[Transport, _ProtocolT]: ...
    else:
        async def start_tls(
            self,
            transport: BaseTransport,
            protocol: BaseProtocol,
            sslcontext: ssl.SSLContext,
            *,
            server_side: bool = False,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
        ) -> Transport | None:
            """Upgrade transport to TLS.

            Return a new transport that *protocol* should start using
            immediately.
            """

        async def connect_accepted_socket(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            sock: socket,
            *,
            ssl: _SSLContext = None,
            ssl_handshake_timeout: float | None = None,
        ) -> tuple[Transport, _ProtocolT]:
            """Handle an accepted connection.

            This is used by servers that accept connections outside of
            asyncio but that use asyncio to handle connections.

            This method is a coroutine.  When completed, the coroutine
            returns a (transport, protocol) pair.
            """

    async def sock_sendfile(
        self, sock: socket, file: IO[bytes], offset: int = 0, count: int | None = None, *, fallback: bool | None = True
    ) -> int: ...
    async def sendfile(
        self, transport: WriteTransport, file: IO[bytes], offset: int = 0, count: int | None = None, *, fallback: bool = True
    ) -> int:
        """Send a file to transport.

        Return the total number of bytes which were sent.

        The method uses high-performance os.sendfile if available.

        file must be a regular file object opened in binary mode.

        offset tells from where to start reading the file. If specified,
        count is the total number of bytes to transmit as opposed to
        sending the file until EOF is reached. File position is updated on
        return or also in case of error in which case file.tell()
        can be used to figure out the number of bytes
        which were sent.

        fallback set to True makes asyncio to manually read and send
        the file when the platform does not support the sendfile syscall
        (e.g. Windows or SSL socket on Unix).

        Raise SendfileNotAvailableError if the system does not support
        sendfile syscall and fallback is False.
        """
    if sys.version_info >= (3, 11):
        async def create_datagram_endpoint(  # type: ignore[override]
            self,
            protocol_factory: Callable[[], _ProtocolT],
            local_addr: tuple[str, int] | str | None = None,
            remote_addr: tuple[str, int] | str | None = None,
            *,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            reuse_port: bool | None = None,
            allow_broadcast: bool | None = None,
            sock: socket | None = None,
        ) -> tuple[DatagramTransport, _ProtocolT]:
            """Create datagram connection."""
    else:
        async def create_datagram_endpoint(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            local_addr: tuple[str, int] | str | None = None,
            remote_addr: tuple[str, int] | str | None = None,
            *,
            family: int = 0,
            proto: int = 0,
            flags: int = 0,
            reuse_address: bool | None = ...,
            reuse_port: bool | None = None,
            allow_broadcast: bool | None = None,
            sock: socket | None = None,
        ) -> tuple[DatagramTransport, _ProtocolT]:
            """Create datagram connection."""
    # Pipes and subprocesses.
    async def connect_read_pipe(
        self, protocol_factory: Callable[[], _ProtocolT], pipe: Any
    ) -> tuple[ReadTransport, _ProtocolT]: ...
    async def connect_write_pipe(
        self, protocol_factory: Callable[[], _ProtocolT], pipe: Any
    ) -> tuple[WriteTransport, _ProtocolT]: ...
    async def subprocess_shell(
        self,
        protocol_factory: Callable[[], _ProtocolT],
        cmd: bytes | str,
        *,
        stdin: int | IO[Any] | None = -1,
        stdout: int | IO[Any] | None = -1,
        stderr: int | IO[Any] | None = -1,
        universal_newlines: Literal[False] = False,
        shell: Literal[True] = True,
        bufsize: Literal[0] = 0,
        encoding: None = None,
        errors: None = None,
        text: Literal[False] | None = None,
        **kwargs: Any,
    ) -> tuple[SubprocessTransport, _ProtocolT]: ...
    async def subprocess_exec(
        self,
        protocol_factory: Callable[[], _ProtocolT],
        program: Any,
        *args: Any,
        stdin: int | IO[Any] | None = -1,
        stdout: int | IO[Any] | None = -1,
        stderr: int | IO[Any] | None = -1,
        universal_newlines: Literal[False] = False,
        shell: Literal[False] = False,
        bufsize: Literal[0] = 0,
        encoding: None = None,
        errors: None = None,
        text: Literal[False] | None = None,
        **kwargs: Any,
    ) -> tuple[SubprocessTransport, _ProtocolT]: ...
    def add_reader(self, fd: FileDescriptorLike, callback: Callable[[Unpack[_Ts]], Any], *args: Unpack[_Ts]) -> None: ...
    def remove_reader(self, fd: FileDescriptorLike) -> bool: ...
    def add_writer(self, fd: FileDescriptorLike, callback: Callable[[Unpack[_Ts]], Any], *args: Unpack[_Ts]) -> None: ...
    def remove_writer(self, fd: FileDescriptorLike) -> bool: ...
    # The sock_* methods (and probably some others) are not actually implemented on
    # BaseEventLoop, only on subclasses. We list them here for now for convenience.
    async def sock_recv(self, sock: socket, nbytes: int) -> bytes: ...
    async def sock_recv_into(self, sock: socket, buf: WriteableBuffer) -> int: ...
    async def sock_sendall(self, sock: socket, data: ReadableBuffer) -> None: ...
    async def sock_connect(self, sock: socket, address: _Address) -> None: ...
    async def sock_accept(self, sock: socket) -> tuple[socket, _RetAddress]: ...
    if sys.version_info >= (3, 11):
        async def sock_recvfrom(self, sock: socket, bufsize: int) -> tuple[bytes, _RetAddress]: ...
        async def sock_recvfrom_into(self, sock: socket, buf: WriteableBuffer, nbytes: int = 0) -> tuple[int, _RetAddress]: ...
        async def sock_sendto(self, sock: socket, data: ReadableBuffer, address: _Address) -> int: ...
    # Signal handling.
    def add_signal_handler(self, sig: int, callback: Callable[[Unpack[_Ts]], Any], *args: Unpack[_Ts]) -> None: ...
    def remove_signal_handler(self, sig: int) -> bool: ...
    # Error handlers.
    def set_exception_handler(self, handler: _ExceptionHandler | None) -> None:
        """Set handler as the new event loop exception handler.

        If handler is None, the default exception handler will
        be set.

        If handler is a callable object, it should have a
        signature matching '(loop, context)', where 'loop'
        will be a reference to the active event loop, 'context'
        will be a dict object (see `call_exception_handler()`
        documentation for details about context).
        """

    def get_exception_handler(self) -> _ExceptionHandler | None:
        """Return an exception handler, or None if the default one is in use."""

    def default_exception_handler(self, context: _Context) -> None:
        """Default exception handler.

        This is called when an exception occurs and no exception
        handler is set, and can be called by a custom exception
        handler that wants to defer to the default behavior.

        This default handler logs the error message and other
        context-dependent information.  In debug mode, a truncated
        stack trace is also appended showing where the given object
        (e.g. a handle or future or task) was created, if any.

        The context parameter has the same meaning as in
        `call_exception_handler()`.
        """

    def call_exception_handler(self, context: _Context) -> None:
        """Call the current event loop's exception handler.

        The context argument is a dict containing the following keys:

        - 'message': Error message;
        - 'exception' (optional): Exception object;
        - 'future' (optional): Future instance;
        - 'task' (optional): Task instance;
        - 'handle' (optional): Handle instance;
        - 'protocol' (optional): Protocol instance;
        - 'transport' (optional): Transport instance;
        - 'socket' (optional): Socket instance;
        - 'source_traceback' (optional): Traceback of the source;
        - 'handle_traceback' (optional): Traceback of the handle;
        - 'asyncgen' (optional): Asynchronous generator that caused
                                 the exception.

        New keys maybe introduced in the future.

        Note: do not overload this method in an event loop subclass.
        For custom exception handling, use the
        `set_exception_handler()` method.
        """
    # Debug flag management.
    def get_debug(self) -> bool: ...
    def set_debug(self, enabled: bool) -> None: ...
    if sys.version_info >= (3, 12):
        async def shutdown_default_executor(self, timeout: float | None = None) -> None:
            """Schedule the shutdown of the default executor.

            The timeout parameter specifies the amount of time the executor will
            be given to finish joining. The default value is None, which means
            that the executor will be given an unlimited amount of time.
            """
    else:
        async def shutdown_default_executor(self) -> None:
            """Schedule the shutdown of the default executor."""

    def __del__(self) -> None: ...
