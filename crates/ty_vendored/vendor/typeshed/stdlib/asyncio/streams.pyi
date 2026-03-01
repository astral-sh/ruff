import ssl
import sys
from _typeshed import ReadableBuffer, StrPath
from collections.abc import Awaitable, Callable, Iterable, Sequence, Sized
from types import ModuleType
from typing import Any, Protocol, SupportsIndex, type_check_only
from typing_extensions import Self, TypeAlias

from . import events, protocols, transports
from .base_events import Server

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.platform == "win32":
    __all__ = ("StreamReader", "StreamWriter", "StreamReaderProtocol", "open_connection", "start_server")
else:
    __all__ = (
        "StreamReader",
        "StreamWriter",
        "StreamReaderProtocol",
        "open_connection",
        "start_server",
        "open_unix_connection",
        "start_unix_server",
    )

_ClientConnectedCallback: TypeAlias = Callable[[StreamReader, StreamWriter], Awaitable[None] | None]

@type_check_only
class _ReaduntilBuffer(ReadableBuffer, Sized, Protocol): ...

if sys.version_info >= (3, 10):
    async def open_connection(
        host: str | None = None,
        port: int | str | None = None,
        *,
        limit: int = 65536,
        ssl_handshake_timeout: float | None = None,
        **kwds: Any,
    ) -> tuple[StreamReader, StreamWriter]:
        """A wrapper for create_connection() returning a (reader, writer) pair.

        The reader returned is a StreamReader instance; the writer is a
        StreamWriter instance.

        The arguments are all the usual arguments to create_connection()
        except protocol_factory; most common are positional host and port,
        with various optional keyword arguments following.

        Additional optional keyword arguments are loop (to set the event loop
        instance to use) and limit (to set the buffer limit passed to the
        StreamReader).

        (If you want to customize the StreamReader and/or
        StreamReaderProtocol classes, just copy the code -- there's
        really nothing special here except some convenience.)
        """

    async def start_server(
        client_connected_cb: _ClientConnectedCallback,
        host: str | Sequence[str] | None = None,
        port: int | str | None = None,
        *,
        limit: int = 65536,
        ssl_handshake_timeout: float | None = None,
        **kwds: Any,
    ) -> Server:
        """Start a socket server, call back for each client connected.

        The first parameter, `client_connected_cb`, takes two parameters:
        client_reader, client_writer.  client_reader is a StreamReader
        object, while client_writer is a StreamWriter object.  This
        parameter can either be a plain callback function or a coroutine;
        if it is a coroutine, it will be automatically converted into a
        Task.

        The rest of the arguments are all the usual arguments to
        loop.create_server() except protocol_factory; most common are
        positional host and port, with various optional keyword arguments
        following.  The return value is the same as loop.create_server().

        Additional optional keyword argument is limit (to set the buffer
        limit passed to the StreamReader).

        The return value is the same as loop.create_server(), i.e. a
        Server object which can be used to stop the service.
        """

else:
    async def open_connection(
        host: str | None = None,
        port: int | str | None = None,
        *,
        loop: events.AbstractEventLoop | None = None,
        limit: int = 65536,
        ssl_handshake_timeout: float | None = None,
        **kwds: Any,
    ) -> tuple[StreamReader, StreamWriter]:
        """A wrapper for create_connection() returning a (reader, writer) pair.

        The reader returned is a StreamReader instance; the writer is a
        StreamWriter instance.

        The arguments are all the usual arguments to create_connection()
        except protocol_factory; most common are positional host and port,
        with various optional keyword arguments following.

        Additional optional keyword arguments are loop (to set the event loop
        instance to use) and limit (to set the buffer limit passed to the
        StreamReader).

        (If you want to customize the StreamReader and/or
        StreamReaderProtocol classes, just copy the code -- there's
        really nothing special here except some convenience.)
        """

    async def start_server(
        client_connected_cb: _ClientConnectedCallback,
        host: str | None = None,
        port: int | str | None = None,
        *,
        loop: events.AbstractEventLoop | None = None,
        limit: int = 65536,
        ssl_handshake_timeout: float | None = None,
        **kwds: Any,
    ) -> Server:
        """Start a socket server, call back for each client connected.

        The first parameter, `client_connected_cb`, takes two parameters:
        client_reader, client_writer.  client_reader is a StreamReader
        object, while client_writer is a StreamWriter object.  This
        parameter can either be a plain callback function or a coroutine;
        if it is a coroutine, it will be automatically converted into a
        Task.

        The rest of the arguments are all the usual arguments to
        loop.create_server() except protocol_factory; most common are
        positional host and port, with various optional keyword arguments
        following.  The return value is the same as loop.create_server().

        Additional optional keyword arguments are loop (to set the event loop
        instance to use) and limit (to set the buffer limit passed to the
        StreamReader).

        The return value is the same as loop.create_server(), i.e. a
        Server object which can be used to stop the service.
        """

if sys.platform != "win32":
    if sys.version_info >= (3, 10):
        async def open_unix_connection(
            path: StrPath | None = None, *, limit: int = 65536, **kwds: Any
        ) -> tuple[StreamReader, StreamWriter]:
            """Similar to `open_connection` but works with UNIX Domain Sockets."""

        async def start_unix_server(
            client_connected_cb: _ClientConnectedCallback, path: StrPath | None = None, *, limit: int = 65536, **kwds: Any
        ) -> Server:
            """Similar to `start_server` but works with UNIX Domain Sockets."""
    else:
        async def open_unix_connection(
            path: StrPath | None = None, *, loop: events.AbstractEventLoop | None = None, limit: int = 65536, **kwds: Any
        ) -> tuple[StreamReader, StreamWriter]:
            """Similar to `open_connection` but works with UNIX Domain Sockets."""

        async def start_unix_server(
            client_connected_cb: _ClientConnectedCallback,
            path: StrPath | None = None,
            *,
            loop: events.AbstractEventLoop | None = None,
            limit: int = 65536,
            **kwds: Any,
        ) -> Server:
            """Similar to `start_server` but works with UNIX Domain Sockets."""

class FlowControlMixin(protocols.Protocol):
    """Reusable flow control logic for StreamWriter.drain().

    This implements the protocol methods pause_writing(),
    resume_writing() and connection_lost().  If the subclass overrides
    these it must call the super methods.

    StreamWriter.drain() must wait for _drain_helper() coroutine.
    """

    def __init__(self, loop: events.AbstractEventLoop | None = None) -> None: ...

class StreamReaderProtocol(FlowControlMixin, protocols.Protocol):
    """Helper class to adapt between Protocol and StreamReader.

    (This is a helper class instead of making StreamReader itself a
    Protocol subclass, because the StreamReader has other potential
    uses, and to prevent the user of the StreamReader to accidentally
    call inappropriate methods of the protocol.)
    """

    def __init__(
        self,
        stream_reader: StreamReader,
        client_connected_cb: _ClientConnectedCallback | None = None,
        loop: events.AbstractEventLoop | None = None,
    ) -> None: ...
    def __del__(self) -> None: ...

class StreamWriter:
    """Wraps a Transport.

    This exposes write(), writelines(), [can_]write_eof(),
    get_extra_info() and close().  It adds drain() which returns an
    optional Future on which you can wait for flow control.  It also
    adds a transport property which references the Transport
    directly.
    """

    def __init__(
        self,
        transport: transports.WriteTransport,
        protocol: protocols.BaseProtocol,
        reader: StreamReader | None,
        loop: events.AbstractEventLoop,
    ) -> None: ...
    @property
    def transport(self) -> transports.WriteTransport: ...
    def write(self, data: bytes | bytearray | memoryview) -> None: ...
    def writelines(self, data: Iterable[bytes | bytearray | memoryview]) -> None: ...
    def write_eof(self) -> None: ...
    def can_write_eof(self) -> bool: ...
    def close(self) -> None: ...
    def is_closing(self) -> bool: ...
    async def wait_closed(self) -> None: ...
    def get_extra_info(self, name: str, default: Any = None) -> Any: ...
    async def drain(self) -> None:
        """Flush the write buffer.

        The intended use is to write

          w.write(data)
          await w.drain()
        """
    if sys.version_info >= (3, 12):
        async def start_tls(
            self,
            sslcontext: ssl.SSLContext,
            *,
            server_hostname: str | None = None,
            ssl_handshake_timeout: float | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> None:
            """Upgrade an existing stream-based connection to TLS."""
    elif sys.version_info >= (3, 11):
        async def start_tls(
            self, sslcontext: ssl.SSLContext, *, server_hostname: str | None = None, ssl_handshake_timeout: float | None = None
        ) -> None:
            """Upgrade an existing stream-based connection to TLS."""
    if sys.version_info >= (3, 13):
        def __del__(self, warnings: ModuleType = ...) -> None: ...
    elif sys.version_info >= (3, 11):
        def __del__(self) -> None: ...

class StreamReader:
    def __init__(self, limit: int = 65536, loop: events.AbstractEventLoop | None = None) -> None: ...
    def exception(self) -> Exception: ...
    def set_exception(self, exc: Exception) -> None: ...
    def set_transport(self, transport: transports.BaseTransport) -> None: ...
    def feed_eof(self) -> None: ...
    def at_eof(self) -> bool:
        """Return True if the buffer is empty and 'feed_eof' was called."""

    def feed_data(self, data: Iterable[SupportsIndex]) -> None: ...
    async def readline(self) -> bytes:
        """Read chunk of data from the stream until newline (b'
        ') is found.

                On success, return chunk that ends with newline. If only partial
                line can be read due to EOF, return incomplete line without
                terminating newline. When EOF was reached while no bytes read, empty
                bytes object is returned.

                If limit is reached, ValueError will be raised. In that case, if
                newline was found, complete line including newline will be removed
                from internal buffer. Else, internal buffer will be cleared. Limit is
                compared against part of the line without newline.

                If stream was paused, this function will automatically resume it if
                needed.
        """
    if sys.version_info >= (3, 13):
        async def readuntil(self, separator: _ReaduntilBuffer | tuple[_ReaduntilBuffer, ...] = b"\n") -> bytes:
            """Read data from the stream until ``separator`` is found.

            On success, the data and separator will be removed from the
            internal buffer (consumed). Returned data will include the
            separator at the end.

            Configured stream limit is used to check result. Limit sets the
            maximal length of data that can be returned, not counting the
            separator.

            If an EOF occurs and the complete separator is still not found,
            an IncompleteReadError exception will be raised, and the internal
            buffer will be reset.  The IncompleteReadError.partial attribute
            may contain the separator partially.

            If the data cannot be read because of over limit, a
            LimitOverrunError exception  will be raised, and the data
            will be left in the internal buffer, so it can be read again.

            The ``separator`` may also be a tuple of separators. In this
            case the return value will be the shortest possible that has any
            separator as the suffix. For the purposes of LimitOverrunError,
            the shortest possible separator is considered to be the one that
            matched.
            """
    else:
        async def readuntil(self, separator: _ReaduntilBuffer = b"\n") -> bytes:
            """Read data from the stream until ``separator`` is found.

            On success, the data and separator will be removed from the
            internal buffer (consumed). Returned data will include the
            separator at the end.

            Configured stream limit is used to check result. Limit sets the
            maximal length of data that can be returned, not counting the
            separator.

            If an EOF occurs and the complete separator is still not found,
            an IncompleteReadError exception will be raised, and the internal
            buffer will be reset.  The IncompleteReadError.partial attribute
            may contain the separator partially.

            If the data cannot be read because of over limit, a
            LimitOverrunError exception  will be raised, and the data
            will be left in the internal buffer, so it can be read again.
            """

    async def read(self, n: int = -1) -> bytes:
        """Read up to `n` bytes from the stream.

        If `n` is not provided or set to -1,
        read until EOF, then return all read bytes.
        If EOF was received and the internal buffer is empty,
        return an empty bytes object.

        If `n` is 0, return an empty bytes object immediately.

        If `n` is positive, return at most `n` available bytes
        as soon as at least 1 byte is available in the internal buffer.
        If EOF is received before any byte is read, return an empty
        bytes object.

        Returned value is not limited with limit, configured at stream
        creation.

        If stream was paused, this function will automatically resume it if
        needed.
        """

    async def readexactly(self, n: int) -> bytes:
        """Read exactly `n` bytes.

        Raise an IncompleteReadError if EOF is reached before `n` bytes can be
        read. The IncompleteReadError.partial attribute of the exception will
        contain the partial read bytes.

        if n is zero, return empty bytes object.

        Returned value is not limited with limit, configured at stream
        creation.

        If stream was paused, this function will automatically resume it if
        needed.
        """

    def __aiter__(self) -> Self: ...
    async def __anext__(self) -> bytes: ...
