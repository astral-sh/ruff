"""Event loop and event loop policy."""

import ssl
import sys
from _asyncio import (
    _get_running_loop as _get_running_loop,
    _set_running_loop as _set_running_loop,
    get_event_loop as get_event_loop,
    get_running_loop as get_running_loop,
)
from _typeshed import FileDescriptorLike, ReadableBuffer, StrPath, Unused, WriteableBuffer
from abc import ABCMeta, abstractmethod
from collections.abc import Callable, Sequence
from concurrent.futures import Executor
from contextvars import Context
from socket import AddressFamily, AddressInfo, SocketKind, _Address, _RetAddress, socket
from typing import IO, Any, Literal, Protocol, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias, TypeVarTuple, Unpack, deprecated

from . import _AwaitableLike, _CoroutineLike
from .base_events import Server
from .futures import Future
from .protocols import BaseProtocol
from .tasks import Task
from .transports import BaseTransport, DatagramTransport, ReadTransport, SubprocessTransport, Transport, WriteTransport

if sys.version_info < (3, 14):
    from .unix_events import AbstractChildWatcher

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 14):
    __all__ = (
        "AbstractEventLoop",
        "AbstractServer",
        "Handle",
        "TimerHandle",
        "get_event_loop_policy",
        "set_event_loop_policy",
        "get_event_loop",
        "set_event_loop",
        "new_event_loop",
        "_set_running_loop",
        "get_running_loop",
        "_get_running_loop",
    )
else:
    __all__ = (
        "AbstractEventLoopPolicy",
        "AbstractEventLoop",
        "AbstractServer",
        "Handle",
        "TimerHandle",
        "get_event_loop_policy",
        "set_event_loop_policy",
        "get_event_loop",
        "set_event_loop",
        "new_event_loop",
        "get_child_watcher",
        "set_child_watcher",
        "_set_running_loop",
        "get_running_loop",
        "_get_running_loop",
    )

_T = TypeVar("_T")
_Ts = TypeVarTuple("_Ts")
_ProtocolT = TypeVar("_ProtocolT", bound=BaseProtocol)
_Context: TypeAlias = dict[str, Any]
_ExceptionHandler: TypeAlias = Callable[[AbstractEventLoop, _Context], object]
_ProtocolFactory: TypeAlias = Callable[[], BaseProtocol]
_SSLContext: TypeAlias = bool | None | ssl.SSLContext

@type_check_only
class _TaskFactory(Protocol):
    def __call__(self, loop: AbstractEventLoop, factory: _CoroutineLike[_T], /) -> Future[_T]: ...

class Handle:
    """Object returned by callback registration methods."""

    __slots__ = ("_callback", "_args", "_cancelled", "_loop", "_source_traceback", "_repr", "__weakref__", "_context")
    _cancelled: bool
    _args: Sequence[Any]
    def __init__(
        self, callback: Callable[..., object], args: Sequence[Any], loop: AbstractEventLoop, context: Context | None = None
    ) -> None: ...
    def cancel(self) -> None: ...
    def _run(self) -> None: ...
    def cancelled(self) -> bool: ...
    if sys.version_info >= (3, 12):
        def get_context(self) -> Context: ...

class TimerHandle(Handle):
    """Object returned by timed callback registration methods."""

    __slots__ = ["_scheduled", "_when"]
    def __init__(
        self,
        when: float,
        callback: Callable[..., object],
        args: Sequence[Any],
        loop: AbstractEventLoop,
        context: Context | None = None,
    ) -> None: ...
    def __hash__(self) -> int: ...
    def when(self) -> float:
        """Return a scheduled callback time.

        The time is an absolute timestamp, using the same time
        reference as loop.time().
        """

    def __lt__(self, other: TimerHandle) -> bool: ...
    def __le__(self, other: TimerHandle) -> bool: ...
    def __gt__(self, other: TimerHandle) -> bool: ...
    def __ge__(self, other: TimerHandle) -> bool: ...
    def __eq__(self, other: object) -> bool: ...

class AbstractServer:
    """Abstract server returned by create_server()."""

    @abstractmethod
    def close(self) -> None:
        """Stop serving.  This leaves existing connections open."""
    if sys.version_info >= (3, 13):
        @abstractmethod
        def close_clients(self) -> None:
            """Close all active connections."""

        @abstractmethod
        def abort_clients(self) -> None:
            """Close all active connections immediately."""

    async def __aenter__(self) -> Self: ...
    async def __aexit__(self, *exc: Unused) -> None: ...
    @abstractmethod
    def get_loop(self) -> AbstractEventLoop:
        """Get the event loop the Server object is attached to."""

    @abstractmethod
    def is_serving(self) -> bool:
        """Return True if the server is accepting connections."""

    @abstractmethod
    async def start_serving(self) -> None:
        """Start accepting connections.

        This method is idempotent, so it can be called when
        the server is already being serving.
        """

    @abstractmethod
    async def serve_forever(self) -> None:
        """Start accepting connections until the coroutine is cancelled.

        The server is closed when the coroutine is cancelled.
        """

    @abstractmethod
    async def wait_closed(self) -> None:
        """Coroutine to wait until service is closed."""

class AbstractEventLoop:
    """Abstract event loop."""

    slow_callback_duration: float
    @abstractmethod
    def run_forever(self) -> None:
        """Run the event loop until stop() is called."""

    @abstractmethod
    def run_until_complete(self, future: _AwaitableLike[_T]) -> _T:
        """Run the event loop until a Future is done.

        Return the Future's result, or raise its exception.
        """

    @abstractmethod
    def stop(self) -> None:
        """Stop the event loop as soon as reasonable.

        Exactly how soon that is may depend on the implementation, but
        no more I/O callbacks should be scheduled.
        """

    @abstractmethod
    def is_running(self) -> bool:
        """Return whether the event loop is currently running."""

    @abstractmethod
    def is_closed(self) -> bool:
        """Returns True if the event loop was closed."""

    @abstractmethod
    def close(self) -> None:
        """Close the loop.

        The loop should not be running.

        This is idempotent and irreversible.

        No other methods should be called after this one.
        """

    @abstractmethod
    async def shutdown_asyncgens(self) -> None:
        """Shutdown all active asynchronous generators."""
    # Methods scheduling callbacks.  All these return Handles.
    # "context" added in 3.9.10/3.10.2 for call_*
    @abstractmethod
    def call_soon(
        self, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> Handle: ...
    @abstractmethod
    def call_later(
        self, delay: float, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> TimerHandle: ...
    @abstractmethod
    def call_at(
        self, when: float, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> TimerHandle: ...
    @abstractmethod
    def time(self) -> float: ...
    # Future methods
    @abstractmethod
    def create_future(self) -> Future[Any]: ...
    # Tasks methods
    if sys.version_info >= (3, 11):
        @abstractmethod
        def create_task(
            self, coro: _CoroutineLike[_T], *, name: str | None = None, context: Context | None = None
        ) -> Task[_T]: ...
    else:
        @abstractmethod
        def create_task(self, coro: _CoroutineLike[_T], *, name: str | None = None) -> Task[_T]: ...

    @abstractmethod
    def set_task_factory(self, factory: _TaskFactory | None) -> None: ...
    @abstractmethod
    def get_task_factory(self) -> _TaskFactory | None: ...
    # Methods for interacting with threads
    # "context" added in 3.9.10/3.10.2
    @abstractmethod
    def call_soon_threadsafe(
        self, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts], context: Context | None = None
    ) -> Handle: ...
    @abstractmethod
    def run_in_executor(self, executor: Executor | None, func: Callable[[Unpack[_Ts]], _T], *args: Unpack[_Ts]) -> Future[_T]: ...
    @abstractmethod
    def set_default_executor(self, executor: Executor) -> None: ...
    # Network I/O methods returning Futures.
    @abstractmethod
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
    @abstractmethod
    async def getnameinfo(self, sockaddr: tuple[str, int] | tuple[str, int, int, int], flags: int = 0) -> tuple[str, str]: ...
    if sys.version_info >= (3, 11):
        @overload
        @abstractmethod
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
        ) -> tuple[Transport, _ProtocolT]: ...
        @overload
        @abstractmethod
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
        @abstractmethod
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
        ) -> tuple[Transport, _ProtocolT]: ...
        @overload
        @abstractmethod
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
        @abstractmethod
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
            keep_alive: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server:
            """A coroutine which creates a TCP server bound to host and port.

            The return value is a Server object which can be used to stop
            the service.

            If host is an empty string or None all interfaces are assumed
            and a list of multiple sockets will be returned (most likely
            one for IPv4 and another one for IPv6). The host parameter can also be
            a sequence (e.g. list) of hosts to bind to.

            family can be set to either AF_INET or AF_INET6 to force the
            socket to use IPv4 or IPv6. If not set it will be determined
            from host (defaults to AF_UNSPEC).

            flags is a bitmask for getaddrinfo().

            sock can optionally be specified in order to use a preexisting
            socket object.

            backlog is the maximum number of queued connections passed to
            listen() (defaults to 100).

            ssl can be set to an SSLContext to enable SSL over the
            accepted connections.

            reuse_address tells the kernel to reuse a local socket in
            TIME_WAIT state, without waiting for its natural timeout to
            expire. If not specified will automatically be set to True on
            UNIX.

            reuse_port tells the kernel to allow this endpoint to be bound to
            the same port as other existing endpoints are bound to, so long as
            they all set this flag when being created. This option is not
            supported on Windows.

            keep_alive set to True keeps connections active by enabling the
            periodic transmission of messages.

            ssl_handshake_timeout is the time in seconds that an SSL server
            will wait for completion of the SSL handshake before aborting the
            connection. Default is 60s.

            ssl_shutdown_timeout is the time in seconds that an SSL server
            will wait for completion of the SSL shutdown procedure
            before aborting the connection. Default is 30s.

            start_serving set to True (default) causes the created server
            to start accepting connections immediately.  When set to False,
            the user should await Server.start_serving() or Server.serve_forever()
            to make the server to start accepting connections.
            """

        @overload
        @abstractmethod
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
            keep_alive: bool | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server: ...
    elif sys.version_info >= (3, 11):
        @overload
        @abstractmethod
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
            """A coroutine which creates a TCP server bound to host and port.

            The return value is a Server object which can be used to stop
            the service.

            If host is an empty string or None all interfaces are assumed
            and a list of multiple sockets will be returned (most likely
            one for IPv4 and another one for IPv6). The host parameter can also be
            a sequence (e.g. list) of hosts to bind to.

            family can be set to either AF_INET or AF_INET6 to force the
            socket to use IPv4 or IPv6. If not set it will be determined
            from host (defaults to AF_UNSPEC).

            flags is a bitmask for getaddrinfo().

            sock can optionally be specified in order to use a preexisting
            socket object.

            backlog is the maximum number of queued connections passed to
            listen() (defaults to 100).

            ssl can be set to an SSLContext to enable SSL over the
            accepted connections.

            reuse_address tells the kernel to reuse a local socket in
            TIME_WAIT state, without waiting for its natural timeout to
            expire. If not specified will automatically be set to True on
            UNIX.

            reuse_port tells the kernel to allow this endpoint to be bound to
            the same port as other existing endpoints are bound to, so long as
            they all set this flag when being created. This option is not
            supported on Windows.

            ssl_handshake_timeout is the time in seconds that an SSL server
            will wait for completion of the SSL handshake before aborting the
            connection. Default is 60s.

            ssl_shutdown_timeout is the time in seconds that an SSL server
            will wait for completion of the SSL shutdown procedure
            before aborting the connection. Default is 30s.

            start_serving set to True (default) causes the created server
            to start accepting connections immediately.  When set to False,
            the user should await Server.start_serving() or Server.serve_forever()
            to make the server to start accepting connections.
            """

        @overload
        @abstractmethod
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
        @abstractmethod
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
            """A coroutine which creates a TCP server bound to host and port.

            The return value is a Server object which can be used to stop
            the service.

            If host is an empty string or None all interfaces are assumed
            and a list of multiple sockets will be returned (most likely
            one for IPv4 and another one for IPv6). The host parameter can also be
            a sequence (e.g. list) of hosts to bind to.

            family can be set to either AF_INET or AF_INET6 to force the
            socket to use IPv4 or IPv6. If not set it will be determined
            from host (defaults to AF_UNSPEC).

            flags is a bitmask for getaddrinfo().

            sock can optionally be specified in order to use a preexisting
            socket object.

            backlog is the maximum number of queued connections passed to
            listen() (defaults to 100).

            ssl can be set to an SSLContext to enable SSL over the
            accepted connections.

            reuse_address tells the kernel to reuse a local socket in
            TIME_WAIT state, without waiting for its natural timeout to
            expire. If not specified will automatically be set to True on
            UNIX.

            reuse_port tells the kernel to allow this endpoint to be bound to
            the same port as other existing endpoints are bound to, so long as
            they all set this flag when being created. This option is not
            supported on Windows.

            ssl_handshake_timeout is the time in seconds that an SSL server
            will wait for completion of the SSL handshake before aborting the
            connection. Default is 60s.

            start_serving set to True (default) causes the created server
            to start accepting connections immediately.  When set to False,
            the user should await Server.start_serving() or Server.serve_forever()
            to make the server to start accepting connections.
            """

        @overload
        @abstractmethod
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
        @abstractmethod
        async def start_tls(
            self,
            transport: WriteTransport,
            protocol: BaseProtocol,
            sslcontext: ssl.SSLContext,
            *,
            server_side: bool = False,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> Transport | None:
            """Upgrade a transport to TLS.

            Return a new transport that *protocol* should start using
            immediately.
            """

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
        ) -> Server:
            """A coroutine which creates a UNIX Domain Socket server.

            The return value is a Server object, which can be used to stop
            the service.

            path is a str, representing a file system path to bind the
            server socket to.

            sock can optionally be specified in order to use a preexisting
            socket object.

            backlog is the maximum number of queued connections passed to
            listen() (defaults to 100).

            ssl can be set to an SSLContext to enable SSL over the
            accepted connections.

            ssl_handshake_timeout is the time in seconds that an SSL server
            will wait for the SSL handshake to complete (defaults to 60s).

            ssl_shutdown_timeout is the time in seconds that an SSL server
            will wait for the SSL shutdown to finish (defaults to 30s).

            start_serving set to True (default) causes the created server
            to start accepting connections immediately.  When set to False,
            the user should await Server.start_serving() or Server.serve_forever()
            to make the server to start accepting connections.
            """
    else:
        @abstractmethod
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
            """Upgrade a transport to TLS.

            Return a new transport that *protocol* should start using
            immediately.
            """

        async def create_unix_server(
            self,
            protocol_factory: _ProtocolFactory,
            path: StrPath | None = None,
            *,
            sock: socket | None = None,
            backlog: int = 100,
            ssl: _SSLContext = None,
            ssl_handshake_timeout: float | None = None,
            start_serving: bool = True,
        ) -> Server:
            """A coroutine which creates a UNIX Domain Socket server.

            The return value is a Server object, which can be used to stop
            the service.

            path is a str, representing a file system path to bind the
            server socket to.

            sock can optionally be specified in order to use a preexisting
            socket object.

            backlog is the maximum number of queued connections passed to
            listen() (defaults to 100).

            ssl can be set to an SSLContext to enable SSL over the
            accepted connections.

            ssl_handshake_timeout is the time in seconds that an SSL server
            will wait for the SSL handshake to complete (defaults to 60s).

            start_serving set to True (default) causes the created server
            to start accepting connections immediately.  When set to False,
            the user should await Server.start_serving() or Server.serve_forever()
            to make the server to start accepting connections.
            """
    if sys.version_info >= (3, 11):
        async def connect_accepted_socket(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            sock: socket,
            *,
            ssl: _SSLContext = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> tuple[Transport, _ProtocolT]:
            """Handle an accepted connection.

            This is used by servers that accept connections outside of
            asyncio, but use asyncio to handle connections.

            This method is a coroutine.  When completed, the coroutine
            returns a (transport, protocol) pair.
            """
    elif sys.version_info >= (3, 10):
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
            asyncio, but use asyncio to handle connections.

            This method is a coroutine.  When completed, the coroutine
            returns a (transport, protocol) pair.
            """
    if sys.version_info >= (3, 11):
        async def create_unix_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            path: str | None = None,
            *,
            ssl: _SSLContext = None,
            sock: socket | None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> tuple[Transport, _ProtocolT]: ...
    else:
        async def create_unix_connection(
            self,
            protocol_factory: Callable[[], _ProtocolT],
            path: str | None = None,
            *,
            ssl: _SSLContext = None,
            sock: socket | None = None,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
        ) -> tuple[Transport, _ProtocolT]: ...

    @abstractmethod
    async def sock_sendfile(
        self, sock: socket, file: IO[bytes], offset: int = 0, count: int | None = None, *, fallback: bool | None = None
    ) -> int: ...
    @abstractmethod
    async def sendfile(
        self, transport: WriteTransport, file: IO[bytes], offset: int = 0, count: int | None = None, *, fallback: bool = True
    ) -> int:
        """Send a file through a transport.

        Return an amount of sent bytes.
        """

    @abstractmethod
    async def create_datagram_endpoint(
        self,
        protocol_factory: Callable[[], _ProtocolT],
        local_addr: tuple[str, int] | str | None = None,
        remote_addr: tuple[str, int] | str | None = None,
        *,
        family: int = 0,
        proto: int = 0,
        flags: int = 0,
        reuse_address: bool | None = None,
        reuse_port: bool | None = None,
        allow_broadcast: bool | None = None,
        sock: socket | None = None,
    ) -> tuple[DatagramTransport, _ProtocolT]:
        """A coroutine which creates a datagram endpoint.

        This method will try to establish the endpoint in the background.
        When successful, the coroutine returns a (transport, protocol) pair.

        protocol_factory must be a callable returning a protocol instance.

        socket family AF_INET, socket.AF_INET6 or socket.AF_UNIX depending on
        host (or family if specified), socket type SOCK_DGRAM.

        reuse_address tells the kernel to reuse a local socket in
        TIME_WAIT state, without waiting for its natural timeout to
        expire. If not specified it will automatically be set to True on
        UNIX.

        reuse_port tells the kernel to allow this endpoint to be bound to
        the same port as other existing endpoints are bound to, so long as
        they all set this flag when being created. This option is not
        supported on Windows and some UNIX's. If the
        :py:data:`~socket.SO_REUSEPORT` constant is not defined then this
        capability is unsupported.

        allow_broadcast tells the kernel to allow this endpoint to send
        messages to the broadcast address.

        sock can optionally be specified in order to use a preexisting
        socket object.
        """
    # Pipes and subprocesses.
    @abstractmethod
    async def connect_read_pipe(self, protocol_factory: Callable[[], _ProtocolT], pipe: Any) -> tuple[ReadTransport, _ProtocolT]:
        """Register read pipe in event loop. Set the pipe to non-blocking mode.

        protocol_factory should instantiate object with Protocol interface.
        pipe is a file-like object.
        Return pair (transport, protocol), where transport supports the
        ReadTransport interface.
        """

    @abstractmethod
    async def connect_write_pipe(
        self, protocol_factory: Callable[[], _ProtocolT], pipe: Any
    ) -> tuple[WriteTransport, _ProtocolT]:
        """Register write pipe in event loop.

        protocol_factory should instantiate object with BaseProtocol interface.
        Pipe is file-like object already switched to nonblocking.
        Return pair (transport, protocol), where transport support
        WriteTransport interface.
        """

    @abstractmethod
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
    @abstractmethod
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
        **kwargs: Any,
    ) -> tuple[SubprocessTransport, _ProtocolT]: ...
    @abstractmethod
    def add_reader(self, fd: FileDescriptorLike, callback: Callable[[Unpack[_Ts]], Any], *args: Unpack[_Ts]) -> None: ...
    @abstractmethod
    def remove_reader(self, fd: FileDescriptorLike) -> bool: ...
    @abstractmethod
    def add_writer(self, fd: FileDescriptorLike, callback: Callable[[Unpack[_Ts]], Any], *args: Unpack[_Ts]) -> None: ...
    @abstractmethod
    def remove_writer(self, fd: FileDescriptorLike) -> bool: ...
    @abstractmethod
    async def sock_recv(self, sock: socket, nbytes: int) -> bytes: ...
    @abstractmethod
    async def sock_recv_into(self, sock: socket, buf: WriteableBuffer) -> int: ...
    @abstractmethod
    async def sock_sendall(self, sock: socket, data: ReadableBuffer) -> None: ...
    @abstractmethod
    async def sock_connect(self, sock: socket, address: _Address) -> None: ...
    @abstractmethod
    async def sock_accept(self, sock: socket) -> tuple[socket, _RetAddress]: ...
    if sys.version_info >= (3, 11):
        @abstractmethod
        async def sock_recvfrom(self, sock: socket, bufsize: int) -> tuple[bytes, _RetAddress]: ...
        @abstractmethod
        async def sock_recvfrom_into(self, sock: socket, buf: WriteableBuffer, nbytes: int = 0) -> tuple[int, _RetAddress]: ...
        @abstractmethod
        async def sock_sendto(self, sock: socket, data: ReadableBuffer, address: _Address) -> int: ...
    # Signal handling.
    @abstractmethod
    def add_signal_handler(self, sig: int, callback: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts]) -> None: ...
    @abstractmethod
    def remove_signal_handler(self, sig: int) -> bool: ...
    # Error handlers.
    @abstractmethod
    def set_exception_handler(self, handler: _ExceptionHandler | None) -> None: ...
    @abstractmethod
    def get_exception_handler(self) -> _ExceptionHandler | None: ...
    @abstractmethod
    def default_exception_handler(self, context: _Context) -> None: ...
    @abstractmethod
    def call_exception_handler(self, context: _Context) -> None: ...
    # Debug flag management.
    @abstractmethod
    def get_debug(self) -> bool: ...
    @abstractmethod
    def set_debug(self, enabled: bool) -> None: ...
    @abstractmethod
    async def shutdown_default_executor(self) -> None:
        """Schedule the shutdown of the default executor."""

if sys.version_info >= (3, 14):
    class _AbstractEventLoopPolicy:
        """Abstract policy for accessing the event loop."""

        @abstractmethod
        def get_event_loop(self) -> AbstractEventLoop:
            """Get the event loop for the current context.

            Returns an event loop object implementing the AbstractEventLoop interface,
            or raises an exception in case no event loop has been set for the
            current context and the current policy does not specify to create one.

            It should never return None.
            """

        @abstractmethod
        def set_event_loop(self, loop: AbstractEventLoop | None) -> None:
            """Set the event loop for the current context to loop."""

        @abstractmethod
        def new_event_loop(self) -> AbstractEventLoop:
            """Create and return a new event loop object according to this
            policy's rules. If there's need to set this loop as the event loop for
            the current context, set_event_loop must be called explicitly.
            """

else:
    @type_check_only
    class _AbstractEventLoopPolicy:
        @abstractmethod
        def get_event_loop(self) -> AbstractEventLoop: ...
        @abstractmethod
        def set_event_loop(self, loop: AbstractEventLoop | None) -> None: ...
        @abstractmethod
        def new_event_loop(self) -> AbstractEventLoop: ...
        # Child processes handling (Unix only).
        if sys.version_info >= (3, 12):
            @abstractmethod
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            def get_child_watcher(self) -> AbstractChildWatcher: ...
            @abstractmethod
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            def set_child_watcher(self, watcher: AbstractChildWatcher) -> None: ...
        else:
            @abstractmethod
            def get_child_watcher(self) -> AbstractChildWatcher: ...
            @abstractmethod
            def set_child_watcher(self, watcher: AbstractChildWatcher) -> None: ...

    AbstractEventLoopPolicy = _AbstractEventLoopPolicy

if sys.version_info >= (3, 14):
    class _BaseDefaultEventLoopPolicy(_AbstractEventLoopPolicy, metaclass=ABCMeta):
        """Default policy implementation for accessing the event loop.

        In this policy, each thread has its own event loop.  However, we
        only automatically create an event loop by default for the main
        thread; other threads by default have no event loop.

        Other policies may have different rules (e.g. a single global
        event loop, or automatically creating an event loop per thread, or
        using some other notion of context to which an event loop is
        associated).
        """

        def get_event_loop(self) -> AbstractEventLoop:
            """Get the event loop for the current context.

            Returns an instance of EventLoop or raises an exception.
            """

        def set_event_loop(self, loop: AbstractEventLoop | None) -> None:
            """Set the event loop."""

        def new_event_loop(self) -> AbstractEventLoop:
            """Create a new event loop.

            You must call set_event_loop() to make this the current event
            loop.
            """

else:
    class BaseDefaultEventLoopPolicy(_AbstractEventLoopPolicy, metaclass=ABCMeta):
        """Default policy implementation for accessing the event loop.

        In this policy, each thread has its own event loop.  However, we
        only automatically create an event loop by default for the main
        thread; other threads by default have no event loop.

        Other policies may have different rules (e.g. a single global
        event loop, or automatically creating an event loop per thread, or
        using some other notion of context to which an event loop is
        associated).
        """

        def get_event_loop(self) -> AbstractEventLoop:
            """Get the event loop for the current context.

            Returns an instance of EventLoop or raises an exception.
            """

        def set_event_loop(self, loop: AbstractEventLoop | None) -> None:
            """Set the event loop."""

        def new_event_loop(self) -> AbstractEventLoop:
            """Create a new event loop.

            You must call set_event_loop() to make this the current event
            loop.
            """

if sys.version_info >= (3, 14):
    def _get_event_loop_policy() -> _AbstractEventLoopPolicy:
        """Get the current event loop policy."""

    def _set_event_loop_policy(policy: _AbstractEventLoopPolicy | None) -> None:
        """Set the current event loop policy.

        If policy is None, the default policy is restored.
        """

    @deprecated("Deprecated since Python 3.14; will be removed in Python 3.16.")
    def get_event_loop_policy() -> _AbstractEventLoopPolicy: ...
    @deprecated("Deprecated since Python 3.14; will be removed in Python 3.16.")
    def set_event_loop_policy(policy: _AbstractEventLoopPolicy | None) -> None: ...

else:
    def get_event_loop_policy() -> _AbstractEventLoopPolicy:
        """Get the current event loop policy."""

    def set_event_loop_policy(policy: _AbstractEventLoopPolicy | None) -> None:
        """Set the current event loop policy.

        If policy is None, the default policy is restored.
        """

def set_event_loop(loop: AbstractEventLoop | None) -> None:
    """Equivalent to calling get_event_loop_policy().set_event_loop(loop)."""

def new_event_loop() -> AbstractEventLoop:
    """Equivalent to calling get_event_loop_policy().new_event_loop()."""

if sys.version_info < (3, 14):
    if sys.version_info >= (3, 12):
        @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
        def get_child_watcher() -> AbstractChildWatcher:
            """Equivalent to calling get_event_loop_policy().get_child_watcher()."""

        @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
        def set_child_watcher(watcher: AbstractChildWatcher) -> None:
            """Equivalent to calling
            get_event_loop_policy().set_child_watcher(watcher).
            """
    else:
        def get_child_watcher() -> AbstractChildWatcher:
            """Equivalent to calling get_event_loop_policy().get_child_watcher()."""

        def set_child_watcher(watcher: AbstractChildWatcher) -> None:
            """Equivalent to calling
            get_event_loop_policy().set_child_watcher(watcher).
            """
