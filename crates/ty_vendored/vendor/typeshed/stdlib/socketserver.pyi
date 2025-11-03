"""Generic socket server classes.

This module tries to capture the various aspects of defining a server:

For socket-based servers:

- address family:
        - AF_INET{,6}: IP (Internet Protocol) sockets (default)
        - AF_UNIX: Unix domain sockets
        - others, e.g. AF_DECNET are conceivable (see <socket.h>
- socket type:
        - SOCK_STREAM (reliable stream, e.g. TCP)
        - SOCK_DGRAM (datagrams, e.g. UDP)

For request-based servers (including socket-based):

- client address verification before further looking at the request
        (This is actually a hook for any processing that needs to look
         at the request before anything else, e.g. logging)
- how to handle multiple requests:
        - synchronous (one request is handled at a time)
        - forking (each request is handled by a new process)
        - threading (each request is handled by a new thread)

The classes in this module favor the server type that is simplest to
write: a synchronous TCP/IP server.  This is bad class design, but
saves some typing.  (There's also the issue that a deep class hierarchy
slows down method lookups.)

There are five classes in an inheritance diagram, four of which represent
synchronous servers of four types:

        +------------+
        | BaseServer |
        +------------+
              |
              v
        +-----------+        +------------------+
        | TCPServer |------->| UnixStreamServer |
        +-----------+        +------------------+
              |
              v
        +-----------+        +--------------------+
        | UDPServer |------->| UnixDatagramServer |
        +-----------+        +--------------------+

Note that UnixDatagramServer derives from UDPServer, not from
UnixStreamServer -- the only difference between an IP and a Unix
stream server is the address family, which is simply repeated in both
unix server classes.

Forking and threading versions of each type of server can be created
using the ForkingMixIn and ThreadingMixIn mix-in classes.  For
instance, a threading UDP server class is created as follows:

        class ThreadingUDPServer(ThreadingMixIn, UDPServer): pass

The Mix-in class must come first, since it overrides a method defined
in UDPServer! Setting the various member variables also changes
the behavior of the underlying server mechanism.

To implement a service, you must derive a class from
BaseRequestHandler and redefine its handle() method.  You can then run
various versions of the service by combining one of the server classes
with your request handler class.

The request handler class must be different for datagram or stream
services.  This can be hidden by using the request handler
subclasses StreamRequestHandler or DatagramRequestHandler.

Of course, you still have to use your head!

For instance, it makes no sense to use a forking server if the service
contains state in memory that can be modified by requests (since the
modifications in the child process would never reach the initial state
kept in the parent process and passed to each child).  In this case,
you can use a threading server, but you will probably have to use
locks to avoid two requests that come in nearly simultaneous to apply
conflicting changes to the server state.

On the other hand, if you are building e.g. an HTTP server, where all
data is stored externally (e.g. in the file system), a synchronous
class will essentially render the service "deaf" while one request is
being handled -- which may be for a very long time if a client is slow
to read all the data it has requested.  Here a threading or forking
server is appropriate.

In some cases, it may be appropriate to process part of a request
synchronously, but to finish processing in a forked child depending on
the request data.  This can be implemented by using a synchronous
server and doing an explicit fork in the request handler class
handle() method.

Another approach to handling multiple simultaneous requests in an
environment that supports neither threads nor fork (or where these are
too expensive or inappropriate for the service) is to maintain an
explicit table of partially finished requests and to use a selector to
decide which request to work on next (or whether to handle a new
incoming request).  This is particularly important for stream services
where each client can potentially be connected for a long time (if
threads or subprocesses cannot be used).

Future work:
- Standard classes for Sun RPC (which uses either UDP or TCP)
- Standard mix-in classes to implement various authentication
  and encryption schemes

XXX Open problems:
- What to do with out-of-band data?

BaseServer:
- split generic "request" functionality out into BaseServer class.
  Copyright (C) 2000  Luke Kenneth Casson Leighton <lkcl@samba.org>

  example: read entries from a SQL database (requires overriding
  get_request() to return a table entry from the database).
  entry is processed by a RequestHandlerClass.

"""

import sys
import types
from _socket import _Address, _RetAddress
from _typeshed import ReadableBuffer
from collections.abc import Callable
from io import BufferedIOBase
from socket import socket as _socket
from typing import Any, ClassVar
from typing_extensions import Self, TypeAlias

__all__ = [
    "BaseServer",
    "TCPServer",
    "UDPServer",
    "ThreadingUDPServer",
    "ThreadingTCPServer",
    "BaseRequestHandler",
    "StreamRequestHandler",
    "DatagramRequestHandler",
    "ThreadingMixIn",
]
if sys.platform != "win32":
    __all__ += [
        "ForkingMixIn",
        "ForkingTCPServer",
        "ForkingUDPServer",
        "ThreadingUnixDatagramServer",
        "ThreadingUnixStreamServer",
        "UnixDatagramServer",
        "UnixStreamServer",
    ]
    if sys.version_info >= (3, 12):
        __all__ += ["ForkingUnixStreamServer", "ForkingUnixDatagramServer"]

_RequestType: TypeAlias = _socket | tuple[bytes, _socket]
_AfUnixAddress: TypeAlias = str | ReadableBuffer  # address acceptable for an AF_UNIX socket
_AfInetAddress: TypeAlias = tuple[str | bytes | bytearray, int]  # address acceptable for an AF_INET socket
_AfInet6Address: TypeAlias = tuple[str | bytes | bytearray, int, int, int]  # address acceptable for an AF_INET6 socket

# This can possibly be generic at some point:
class BaseServer:
    """Base class for server classes.

    Methods for the caller:

    - __init__(server_address, RequestHandlerClass)
    - serve_forever(poll_interval=0.5)
    - shutdown()
    - handle_request()  # if you do not use serve_forever()
    - fileno() -> int   # for selector

    Methods that may be overridden:

    - server_bind()
    - server_activate()
    - get_request() -> request, client_address
    - handle_timeout()
    - verify_request(request, client_address)
    - server_close()
    - process_request(request, client_address)
    - shutdown_request(request)
    - close_request(request)
    - service_actions()
    - handle_error()

    Methods for derived classes:

    - finish_request(request, client_address)

    Class variables that may be overridden by derived classes or
    instances:

    - timeout
    - address_family
    - socket_type
    - allow_reuse_address
    - allow_reuse_port

    Instance variables:

    - RequestHandlerClass
    - socket

    """

    server_address: _Address
    timeout: float | None
    RequestHandlerClass: Callable[[Any, _RetAddress, Self], BaseRequestHandler]
    def __init__(
        self, server_address: _Address, RequestHandlerClass: Callable[[Any, _RetAddress, Self], BaseRequestHandler]
    ) -> None:
        """Constructor.  May be extended, do not override."""

    def handle_request(self) -> None:
        """Handle one request, possibly blocking.

        Respects self.timeout.
        """

    def serve_forever(self, poll_interval: float = 0.5) -> None:
        """Handle one request at a time until shutdown.

        Polls for shutdown every poll_interval seconds. Ignores
        self.timeout. If you need to do periodic tasks, do them in
        another thread.
        """

    def shutdown(self) -> None:
        """Stops the serve_forever loop.

        Blocks until the loop has finished. This must be called while
        serve_forever() is running in another thread, or it will
        deadlock.
        """

    def server_close(self) -> None:
        """Called to clean-up the server.

        May be overridden.

        """

    def finish_request(self, request: _RequestType, client_address: _RetAddress) -> None:
        """Finish one request by instantiating RequestHandlerClass."""

    def get_request(self) -> tuple[Any, Any]: ...  # Not implemented here, but expected to exist on subclasses
    def handle_error(self, request: _RequestType, client_address: _RetAddress) -> None:
        """Handle an error gracefully.  May be overridden.

        The default is to print a traceback and continue.

        """

    def handle_timeout(self) -> None:
        """Called if no new request arrives within self.timeout.

        Overridden by ForkingMixIn.
        """

    def process_request(self, request: _RequestType, client_address: _RetAddress) -> None:
        """Call finish_request.

        Overridden by ForkingMixIn and ThreadingMixIn.

        """

    def server_activate(self) -> None:
        """Called by constructor to activate the server.

        May be overridden.

        """

    def verify_request(self, request: _RequestType, client_address: _RetAddress) -> bool:
        """Verify the request.  May be overridden.

        Return True if we should proceed with this request.

        """

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
    ) -> None: ...
    def service_actions(self) -> None:
        """Called by the serve_forever() loop.

        May be overridden by a subclass / Mixin to implement any code that
        needs to be run during the loop.
        """

    def shutdown_request(self, request: _RequestType) -> None:  # undocumented
        """Called to shutdown and close an individual request."""

    def close_request(self, request: _RequestType) -> None:  # undocumented
        """Called to clean up an individual request."""

class TCPServer(BaseServer):
    """Base class for various socket-based server classes.

    Defaults to synchronous IP stream (i.e., TCP).

    Methods for the caller:

    - __init__(server_address, RequestHandlerClass, bind_and_activate=True)
    - serve_forever(poll_interval=0.5)
    - shutdown()
    - handle_request()  # if you don't use serve_forever()
    - fileno() -> int   # for selector

    Methods that may be overridden:

    - server_bind()
    - server_activate()
    - get_request() -> request, client_address
    - handle_timeout()
    - verify_request(request, client_address)
    - process_request(request, client_address)
    - shutdown_request(request)
    - close_request(request)
    - handle_error()

    Methods for derived classes:

    - finish_request(request, client_address)

    Class variables that may be overridden by derived classes or
    instances:

    - timeout
    - address_family
    - socket_type
    - request_queue_size (only for stream sockets)
    - allow_reuse_address
    - allow_reuse_port

    Instance variables:

    - server_address
    - RequestHandlerClass
    - socket

    """

    address_family: int
    socket: _socket
    allow_reuse_address: bool
    request_queue_size: int
    socket_type: int
    if sys.version_info >= (3, 11):
        allow_reuse_port: bool
    server_address: _AfInetAddress | _AfInet6Address
    def __init__(
        self,
        server_address: _AfInetAddress | _AfInet6Address,
        RequestHandlerClass: Callable[[Any, _RetAddress, Self], BaseRequestHandler],
        bind_and_activate: bool = True,
    ) -> None:
        """Constructor.  May be extended, do not override."""

    def fileno(self) -> int:
        """Return socket file number.

        Interface required by selector.

        """

    def get_request(self) -> tuple[_socket, _RetAddress]:
        """Get the request and client address from the socket.

        May be overridden.

        """

    def server_bind(self) -> None:
        """Called by constructor to bind the socket.

        May be overridden.

        """

class UDPServer(TCPServer):
    """UDP server class."""

    max_packet_size: ClassVar[int]
    def get_request(self) -> tuple[tuple[bytes, _socket], _RetAddress]: ...  # type: ignore[override]

if sys.platform != "win32":
    class UnixStreamServer(TCPServer):
        server_address: _AfUnixAddress  # type: ignore[assignment]
        def __init__(
            self,
            server_address: _AfUnixAddress,
            RequestHandlerClass: Callable[[Any, _RetAddress, Self], BaseRequestHandler],
            bind_and_activate: bool = True,
        ) -> None:
            """Constructor.  May be extended, do not override."""

    class UnixDatagramServer(UDPServer):
        server_address: _AfUnixAddress  # type: ignore[assignment]
        def __init__(
            self,
            server_address: _AfUnixAddress,
            RequestHandlerClass: Callable[[Any, _RetAddress, Self], BaseRequestHandler],
            bind_and_activate: bool = True,
        ) -> None:
            """Constructor.  May be extended, do not override."""

if sys.platform != "win32":
    class ForkingMixIn:
        """Mix-in class to handle each request in a new process."""

        timeout: float | None  # undocumented
        active_children: set[int] | None  # undocumented
        max_children: int  # undocumented
        block_on_close: bool
        def collect_children(self, *, blocking: bool = False) -> None:  # undocumented
            """Internal routine to wait for children that have exited."""

        def handle_timeout(self) -> None:  # undocumented
            """Wait for zombies after self.timeout seconds of inactivity.

            May be extended, do not override.
            """

        def service_actions(self) -> None:  # undocumented
            """Collect the zombie child processes regularly in the ForkingMixIn.

            service_actions is called in the BaseServer's serve_forever loop.
            """

        def process_request(self, request: _RequestType, client_address: _RetAddress) -> None:
            """Fork a new subprocess to process the request."""

        def server_close(self) -> None: ...

class ThreadingMixIn:
    """Mix-in class to handle each request in a new thread."""

    daemon_threads: bool
    block_on_close: bool
    def process_request_thread(self, request: _RequestType, client_address: _RetAddress) -> None:  # undocumented
        """Same as in BaseServer but as a thread.

        In addition, exception handling is done here.

        """

    def process_request(self, request: _RequestType, client_address: _RetAddress) -> None:
        """Start a new thread to process the request."""

    def server_close(self) -> None: ...

if sys.platform != "win32":
    class ForkingTCPServer(ForkingMixIn, TCPServer): ...
    class ForkingUDPServer(ForkingMixIn, UDPServer): ...
    if sys.version_info >= (3, 12):
        class ForkingUnixStreamServer(ForkingMixIn, UnixStreamServer): ...
        class ForkingUnixDatagramServer(ForkingMixIn, UnixDatagramServer): ...

class ThreadingTCPServer(ThreadingMixIn, TCPServer): ...
class ThreadingUDPServer(ThreadingMixIn, UDPServer): ...

if sys.platform != "win32":
    class ThreadingUnixStreamServer(ThreadingMixIn, UnixStreamServer): ...
    class ThreadingUnixDatagramServer(ThreadingMixIn, UnixDatagramServer): ...

class BaseRequestHandler:
    """Base class for request handler classes.

    This class is instantiated for each request to be handled.  The
    constructor sets the instance variables request, client_address
    and server, and then calls the handle() method.  To implement a
    specific service, all you need to do is to derive a class which
    defines a handle() method.

    The handle() method can find the request as self.request, the
    client address as self.client_address, and the server (in case it
    needs access to per-server information) as self.server.  Since a
    separate instance is created for each request, the handle() method
    can define other arbitrary instance variables.

    """

    # `request` is technically of type _RequestType,
    # but there are some concerns that having a union here would cause
    # too much inconvenience to people using it (see
    # https://github.com/python/typeshed/pull/384#issuecomment-234649696)
    #
    # Note also that _RetAddress is also just an alias for `Any`
    request: Any
    client_address: _RetAddress
    server: BaseServer
    def __init__(self, request: _RequestType, client_address: _RetAddress, server: BaseServer) -> None: ...
    def setup(self) -> None: ...
    def handle(self) -> None: ...
    def finish(self) -> None: ...

class StreamRequestHandler(BaseRequestHandler):
    """Define self.rfile and self.wfile for stream sockets."""

    rbufsize: ClassVar[int]  # undocumented
    wbufsize: ClassVar[int]  # undocumented
    timeout: ClassVar[float | None]  # undocumented
    disable_nagle_algorithm: ClassVar[bool]  # undocumented
    connection: Any  # undocumented
    rfile: BufferedIOBase
    wfile: BufferedIOBase

class DatagramRequestHandler(BaseRequestHandler):
    """Define self.rfile and self.wfile for datagram sockets."""

    packet: bytes  # undocumented
    socket: _socket  # undocumented
    rfile: BufferedIOBase
    wfile: BufferedIOBase
