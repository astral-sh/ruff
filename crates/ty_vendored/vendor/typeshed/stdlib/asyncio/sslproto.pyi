import ssl
import sys
from collections import deque
from collections.abc import Callable
from enum import Enum
from typing import Any, ClassVar, Final, Literal
from typing_extensions import TypeAlias

from . import constants, events, futures, protocols, transports

def _create_transport_context(server_side: bool, server_hostname: str | None) -> ssl.SSLContext: ...

if sys.version_info >= (3, 11):
    SSLAgainErrors: tuple[type[ssl.SSLWantReadError], type[ssl.SSLSyscallError]]

    class SSLProtocolState(Enum):
        UNWRAPPED = "UNWRAPPED"
        DO_HANDSHAKE = "DO_HANDSHAKE"
        WRAPPED = "WRAPPED"
        FLUSHING = "FLUSHING"
        SHUTDOWN = "SHUTDOWN"

    class AppProtocolState(Enum):
        STATE_INIT = "STATE_INIT"
        STATE_CON_MADE = "STATE_CON_MADE"
        STATE_EOF = "STATE_EOF"
        STATE_CON_LOST = "STATE_CON_LOST"

    def add_flowcontrol_defaults(high: int | None, low: int | None, kb: int) -> tuple[int, int]: ...

else:
    _UNWRAPPED: Final = "UNWRAPPED"
    _DO_HANDSHAKE: Final = "DO_HANDSHAKE"
    _WRAPPED: Final = "WRAPPED"
    _SHUTDOWN: Final = "SHUTDOWN"

if sys.version_info < (3, 11):
    class _SSLPipe:
        """An SSL "Pipe".

        An SSL pipe allows you to communicate with an SSL/TLS protocol instance
        through memory buffers. It can be used to implement a security layer for an
        existing connection where you don't have access to the connection's file
        descriptor, or for some reason you don't want to use it.

        An SSL pipe can be in "wrapped" and "unwrapped" mode. In unwrapped mode,
        data is passed through untransformed. In wrapped mode, application level
        data is encrypted to SSL record level data and vice versa. The SSL record
        level is the lowest level in the SSL protocol suite and is what travels
        as-is over the wire.

        An SslPipe initially is in "unwrapped" mode. To start SSL, call
        do_handshake(). To shutdown SSL again, call unwrap().
        """

        max_size: ClassVar[int]

        _context: ssl.SSLContext
        _server_side: bool
        _server_hostname: str | None
        _state: str
        _incoming: ssl.MemoryBIO
        _outgoing: ssl.MemoryBIO
        _sslobj: ssl.SSLObject | None
        _need_ssldata: bool
        _handshake_cb: Callable[[BaseException | None], None] | None
        _shutdown_cb: Callable[[], None] | None
        def __init__(self, context: ssl.SSLContext, server_side: bool, server_hostname: str | None = None) -> None:
            """
            The *context* argument specifies the ssl.SSLContext to use.

            The *server_side* argument indicates whether this is a server side or
            client side transport.

            The optional *server_hostname* argument can be used to specify the
            hostname you are connecting to. You may only specify this parameter if
            the _ssl module supports Server Name Indication (SNI).
            """

        @property
        def context(self) -> ssl.SSLContext:
            """The SSL context passed to the constructor."""

        @property
        def ssl_object(self) -> ssl.SSLObject | None:
            """The internal ssl.SSLObject instance.

            Return None if the pipe is not wrapped.
            """

        @property
        def need_ssldata(self) -> bool:
            """Whether more record level data is needed to complete a handshake
            that is currently in progress.
            """

        @property
        def wrapped(self) -> bool:
            """
            Whether a security layer is currently in effect.

            Return False during handshake.
            """

        def do_handshake(self, callback: Callable[[BaseException | None], object] | None = None) -> list[bytes]:
            """Start the SSL handshake.

            Return a list of ssldata. A ssldata element is a list of buffers

            The optional *callback* argument can be used to install a callback that
            will be called when the handshake is complete. The callback will be
            called with None if successful, else an exception instance.
            """

        def shutdown(self, callback: Callable[[], object] | None = None) -> list[bytes]:
            """Start the SSL shutdown sequence.

            Return a list of ssldata. A ssldata element is a list of buffers

            The optional *callback* argument can be used to install a callback that
            will be called when the shutdown is complete. The callback will be
            called without arguments.
            """

        def feed_eof(self) -> None:
            """Send a potentially "ragged" EOF.

            This method will raise an SSL_ERROR_EOF exception if the EOF is
            unexpected.
            """

        def feed_ssldata(self, data: bytes, only_handshake: bool = False) -> tuple[list[bytes], list[bytes]]:
            """Feed SSL record level data into the pipe.

            The data must be a bytes instance. It is OK to send an empty bytes
            instance. This can be used to get ssldata for a handshake initiated by
            this endpoint.

            Return a (ssldata, appdata) tuple. The ssldata element is a list of
            buffers containing SSL data that needs to be sent to the remote SSL.

            The appdata element is a list of buffers containing plaintext data that
            needs to be forwarded to the application. The appdata list may contain
            an empty buffer indicating an SSL "close_notify" alert. This alert must
            be acknowledged by calling shutdown().
            """

        def feed_appdata(self, data: bytes, offset: int = 0) -> tuple[list[bytes], int]:
            """Feed plaintext data into the pipe.

            Return an (ssldata, offset) tuple. The ssldata element is a list of
            buffers containing record level data that needs to be sent to the
            remote SSL instance. The offset is the number of plaintext bytes that
            were processed, which may be less than the length of data.

            NOTE: In case of short writes, this call MUST be retried with the SAME
            buffer passed into the *data* argument (i.e. the id() must be the
            same). This is an OpenSSL requirement. A further particularity is that
            a short write will always have offset == 0, because the _ssl module
            does not enable partial writes. And even though the offset is zero,
            there will still be encrypted data in ssldata.
            """

class _SSLProtocolTransport(transports._FlowControlMixin, transports.Transport):
    _sendfile_compatible: ClassVar[constants._SendfileMode]

    _loop: events.AbstractEventLoop
    if sys.version_info >= (3, 11):
        _ssl_protocol: SSLProtocol | None
    else:
        _ssl_protocol: SSLProtocol
    _closed: bool
    def __init__(self, loop: events.AbstractEventLoop, ssl_protocol: SSLProtocol) -> None: ...
    def get_extra_info(self, name: str, default: Any | None = None) -> dict[str, Any]:
        """Get optional transport information."""

    @property
    def _protocol_paused(self) -> bool: ...
    def write(self, data: bytes | bytearray | memoryview[Any]) -> None:  # any memoryview format or shape
        """Write some data bytes to the transport.

        This does not block; it buffers the data and arranges for it
        to be sent out asynchronously.
        """

    def can_write_eof(self) -> Literal[False]:
        """Return True if this transport supports write_eof(), False if not."""
    if sys.version_info >= (3, 11):
        def get_write_buffer_limits(self) -> tuple[int, int]: ...
        def get_read_buffer_limits(self) -> tuple[int, int]: ...
        def set_read_buffer_limits(self, high: int | None = None, low: int | None = None) -> None:
            """Set the high- and low-water limits for read flow control.

            These two values control when to call the upstream transport's
            pause_reading() and resume_reading() methods.  If specified,
            the low-water limit must be less than or equal to the
            high-water limit.  Neither value can be negative.

            The defaults are implementation-specific.  If only the
            high-water limit is given, the low-water limit defaults to an
            implementation-specific value less than or equal to the
            high-water limit.  Setting high to zero forces low to zero as
            well, and causes pause_reading() to be called whenever the
            buffer becomes non-empty.  Setting low to zero causes
            resume_reading() to be called only once the buffer is empty.
            Use of zero for either limit is generally sub-optimal as it
            reduces opportunities for doing I/O and computation
            concurrently.
            """

        def get_read_buffer_size(self) -> int:
            """Return the current size of the read buffer."""

    def __del__(self) -> None: ...

if sys.version_info >= (3, 11):
    _SSLProtocolBase: TypeAlias = protocols.BufferedProtocol
else:
    _SSLProtocolBase: TypeAlias = protocols.Protocol

class SSLProtocol(_SSLProtocolBase):
    """SSL protocol.

    Implementation of SSL on top of a socket using incoming and outgoing
    buffers which are ssl.MemoryBIO objects.
    """

    _server_side: bool
    _server_hostname: str | None
    _sslcontext: ssl.SSLContext
    _extra: dict[str, Any]
    _write_backlog: deque[tuple[bytes, int]]
    _write_buffer_size: int
    _waiter: futures.Future[Any]
    _loop: events.AbstractEventLoop
    _app_transport: _SSLProtocolTransport
    _transport: transports.BaseTransport | None
    _ssl_handshake_timeout: int | None
    _app_protocol: protocols.BaseProtocol
    _app_protocol_is_buffer: bool

    if sys.version_info >= (3, 11):
        max_size: ClassVar[int]
    else:
        _sslpipe: _SSLPipe | None
        _session_established: bool
        _call_connection_made: bool
        _in_handshake: bool
        _in_shutdown: bool

    if sys.version_info >= (3, 11):
        def __init__(
            self,
            loop: events.AbstractEventLoop,
            app_protocol: protocols.BaseProtocol,
            sslcontext: ssl.SSLContext,
            waiter: futures.Future[Any],
            server_side: bool = False,
            server_hostname: str | None = None,
            call_connection_made: bool = True,
            ssl_handshake_timeout: int | None = None,
            ssl_shutdown_timeout: float | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            loop: events.AbstractEventLoop,
            app_protocol: protocols.BaseProtocol,
            sslcontext: ssl.SSLContext,
            waiter: futures.Future[Any],
            server_side: bool = False,
            server_hostname: str | None = None,
            call_connection_made: bool = True,
            ssl_handshake_timeout: int | None = None,
        ) -> None: ...

    def _set_app_protocol(self, app_protocol: protocols.BaseProtocol) -> None: ...
    def _wakeup_waiter(self, exc: BaseException | None = None) -> None: ...
    def connection_lost(self, exc: BaseException | None) -> None:
        """Called when the low-level connection is lost or closed.

        The argument is an exception object or None (the latter
        meaning a regular EOF is received or the connection was
        aborted or closed).
        """

    def eof_received(self) -> None:
        """Called when the other end of the low-level stream
        is half-closed.

        If this returns a false value (including None), the transport
        will close itself.  If it returns a true value, closing the
        transport is up to the protocol.
        """

    def _get_extra_info(self, name: str, default: Any | None = None) -> Any: ...
    def _start_shutdown(self) -> None: ...
    if sys.version_info >= (3, 11):
        def _write_appdata(self, list_of_data: list[bytes]) -> None: ...
    else:
        def _write_appdata(self, data: bytes) -> None: ...

    def _start_handshake(self) -> None: ...
    def _check_handshake_timeout(self) -> None: ...
    def _on_handshake_complete(self, handshake_exc: BaseException | None) -> None: ...
    def _fatal_error(self, exc: BaseException, message: str = "Fatal error on transport") -> None: ...
    if sys.version_info >= (3, 11):
        def _abort(self, exc: BaseException | None) -> None: ...
        def get_buffer(self, n: int) -> memoryview: ...
    else:
        def _abort(self) -> None: ...
        def _finalize(self) -> None: ...
        def _process_write_backlog(self) -> None: ...
