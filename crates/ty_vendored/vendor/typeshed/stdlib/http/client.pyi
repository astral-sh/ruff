"""HTTP/1.1 client library

<intro stuff goes here>
<other stuff, too>

HTTPConnection goes through a number of "states", which define when a client
may legally make another request or fetch the response for a particular
request. This diagram details these state transitions:

    (null)
      |
      | HTTPConnection()
      v
    Idle
      |
      | putrequest()
      v
    Request-started
      |
      | ( putheader() )*  endheaders()
      v
    Request-sent
      |\\_____________________________
      |                              | getresponse() raises
      | response = getresponse()     | ConnectionError
      v                              v
    Unread-response                Idle
    [Response-headers-read]
      |\\____________________
      |                     |
      | response.read()     | putrequest()
      v                     v
    Idle                  Req-started-unread-response
                     ______/|
                   /        |
   response.read() |        | ( putheader() )*  endheaders()
                   v        v
       Request-started    Req-sent-unread-response
                            |
                            | response.read()
                            v
                          Request-sent

This diagram presents the following rules:
  -- a second request may not be started until {response-headers-read}
  -- a response [object] cannot be retrieved until {request-sent}
  -- there is no differentiation between an unread response body and a
     partially read response body

Note: this enforcement is applied by the HTTPConnection class. The
      HTTPResponse class does not enforce this state machine, which
      implies sophisticated clients may accelerate the request/response
      pipeline. Caution should be taken, though: accelerating the states
      beyond the above pattern may imply knowledge of the server's
      connection-close behavior for certain requests. For example, it
      is impossible to tell whether the server will close the connection
      UNTIL the response headers have been read; this means that further
      requests cannot be placed into the pipeline until it is known that
      the server will NOT be closing the connection.

Logical State                  __state            __response
-------------                  -------            ----------
Idle                           _CS_IDLE           None
Request-started                _CS_REQ_STARTED    None
Request-sent                   _CS_REQ_SENT       None
Unread-response                _CS_IDLE           <response_class>
Req-started-unread-response    _CS_REQ_STARTED    <response_class>
Req-sent-unread-response       _CS_REQ_SENT       <response_class>
"""

import email.message
import io
import ssl
import sys
import types
from _typeshed import MaybeNone, ReadableBuffer, SupportsRead, SupportsReadline, WriteableBuffer
from collections.abc import Callable, Iterable, Iterator, Mapping
from email._policybase import _MessageT
from socket import socket
from typing import BinaryIO, Literal, TypeVar, overload
from typing_extensions import Self, TypeAlias

__all__ = [
    "HTTPResponse",
    "HTTPConnection",
    "HTTPException",
    "NotConnected",
    "UnknownProtocol",
    "UnknownTransferEncoding",
    "UnimplementedFileMode",
    "IncompleteRead",
    "InvalidURL",
    "ImproperConnectionState",
    "CannotSendRequest",
    "CannotSendHeader",
    "ResponseNotReady",
    "BadStatusLine",
    "LineTooLong",
    "RemoteDisconnected",
    "error",
    "responses",
    "HTTPSConnection",
]

_DataType: TypeAlias = SupportsRead[bytes] | Iterable[ReadableBuffer] | ReadableBuffer
_T = TypeVar("_T")
_HeaderValue: TypeAlias = ReadableBuffer | str | int

HTTP_PORT: int
HTTPS_PORT: int

# Keep these global constants in sync with http.HTTPStatus (http/__init__.pyi).
# They are present for backward compatibility reasons.
CONTINUE: Literal[100]
SWITCHING_PROTOCOLS: Literal[101]
PROCESSING: Literal[102]
EARLY_HINTS: Literal[103]

OK: Literal[200]
CREATED: Literal[201]
ACCEPTED: Literal[202]
NON_AUTHORITATIVE_INFORMATION: Literal[203]
NO_CONTENT: Literal[204]
RESET_CONTENT: Literal[205]
PARTIAL_CONTENT: Literal[206]
MULTI_STATUS: Literal[207]
ALREADY_REPORTED: Literal[208]
IM_USED: Literal[226]

MULTIPLE_CHOICES: Literal[300]
MOVED_PERMANENTLY: Literal[301]
FOUND: Literal[302]
SEE_OTHER: Literal[303]
NOT_MODIFIED: Literal[304]
USE_PROXY: Literal[305]
TEMPORARY_REDIRECT: Literal[307]
PERMANENT_REDIRECT: Literal[308]

BAD_REQUEST: Literal[400]
UNAUTHORIZED: Literal[401]
PAYMENT_REQUIRED: Literal[402]
FORBIDDEN: Literal[403]
NOT_FOUND: Literal[404]
METHOD_NOT_ALLOWED: Literal[405]
NOT_ACCEPTABLE: Literal[406]
PROXY_AUTHENTICATION_REQUIRED: Literal[407]
REQUEST_TIMEOUT: Literal[408]
CONFLICT: Literal[409]
GONE: Literal[410]
LENGTH_REQUIRED: Literal[411]
PRECONDITION_FAILED: Literal[412]
if sys.version_info >= (3, 13):
    CONTENT_TOO_LARGE: Literal[413]
REQUEST_ENTITY_TOO_LARGE: Literal[413]
if sys.version_info >= (3, 13):
    URI_TOO_LONG: Literal[414]
REQUEST_URI_TOO_LONG: Literal[414]
UNSUPPORTED_MEDIA_TYPE: Literal[415]
if sys.version_info >= (3, 13):
    RANGE_NOT_SATISFIABLE: Literal[416]
REQUESTED_RANGE_NOT_SATISFIABLE: Literal[416]
EXPECTATION_FAILED: Literal[417]
IM_A_TEAPOT: Literal[418]
MISDIRECTED_REQUEST: Literal[421]
if sys.version_info >= (3, 13):
    UNPROCESSABLE_CONTENT: Literal[422]
UNPROCESSABLE_ENTITY: Literal[422]
LOCKED: Literal[423]
FAILED_DEPENDENCY: Literal[424]
TOO_EARLY: Literal[425]
UPGRADE_REQUIRED: Literal[426]
PRECONDITION_REQUIRED: Literal[428]
TOO_MANY_REQUESTS: Literal[429]
REQUEST_HEADER_FIELDS_TOO_LARGE: Literal[431]
UNAVAILABLE_FOR_LEGAL_REASONS: Literal[451]

INTERNAL_SERVER_ERROR: Literal[500]
NOT_IMPLEMENTED: Literal[501]
BAD_GATEWAY: Literal[502]
SERVICE_UNAVAILABLE: Literal[503]
GATEWAY_TIMEOUT: Literal[504]
HTTP_VERSION_NOT_SUPPORTED: Literal[505]
VARIANT_ALSO_NEGOTIATES: Literal[506]
INSUFFICIENT_STORAGE: Literal[507]
LOOP_DETECTED: Literal[508]
NOT_EXTENDED: Literal[510]
NETWORK_AUTHENTICATION_REQUIRED: Literal[511]

responses: dict[int, str]

class HTTPMessage(email.message.Message[str, str]):
    def getallmatchingheaders(self, name: str) -> list[str]:  # undocumented
        """Find all header lines matching a given header name.

        Look through the list of headers and find all lines matching a given
        header name (and their continuation lines).  A list of the lines is
        returned, without interpretation.  If the header does not occur, an
        empty list is returned.  If the header occurs multiple times, all
        occurrences are returned.  Case is not important in the header name.

        """

@overload
def parse_headers(fp: SupportsReadline[bytes], _class: Callable[[], _MessageT]) -> _MessageT:
    """Parses only RFC2822 headers from a file pointer."""

@overload
def parse_headers(fp: SupportsReadline[bytes]) -> HTTPMessage: ...

class HTTPResponse(io.BufferedIOBase, BinaryIO):  # type: ignore[misc]  # incompatible method definitions in the base classes
    msg: HTTPMessage
    headers: HTTPMessage
    version: int
    debuglevel: int
    fp: io.BufferedReader
    closed: bool
    status: int
    reason: str
    chunked: bool
    chunk_left: int | None
    length: int | None
    will_close: bool
    # url is set on instances of the class in urllib.request.AbstractHTTPHandler.do_open
    # to match urllib.response.addinfourl's interface.
    # It's not set in HTTPResponse.__init__ or any other method on the class
    url: str
    def __init__(self, sock: socket, debuglevel: int = 0, method: str | None = None, url: str | None = None) -> None: ...
    def peek(self, n: int = -1) -> bytes: ...
    def read(self, amt: int | None = None) -> bytes:
        """Read and return the response body, or up to the next amt bytes."""

    def read1(self, n: int = -1) -> bytes:
        """Read with at most one underlying system call.  If at least one
        byte is buffered, return that instead.
        """

    def readinto(self, b: WriteableBuffer) -> int:
        """Read up to len(b) bytes into bytearray b and return the number
        of bytes read.
        """

    def readline(self, limit: int = -1) -> bytes: ...  # type: ignore[override]
    @overload
    def getheader(self, name: str) -> str | None:
        """Returns the value of the header matching *name*.

        If there are multiple matching headers, the values are
        combined into a single string separated by commas and spaces.

        If no matching header is found, returns *default* or None if
        the *default* is not specified.

        If the headers are unknown, raises http.client.ResponseNotReady.

        """

    @overload
    def getheader(self, name: str, default: _T) -> str | _T: ...
    def getheaders(self) -> list[tuple[str, str]]:
        """Return list of (header, value) tuples."""

    def isclosed(self) -> bool:
        """True if the connection is closed."""

    def __iter__(self) -> Iterator[bytes]: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
    ) -> None: ...
    def info(self) -> email.message.Message:
        """Returns an instance of the class mimetools.Message containing
        meta-information associated with the URL.

        When the method is HTTP, these headers are those returned by
        the server at the head of the retrieved HTML page (including
        Content-Length and Content-Type).

        When the method is FTP, a Content-Length header will be
        present if (as is now usual) the server passed back a file
        length in response to the FTP retrieval request. A
        Content-Type header will be present if the MIME type can be
        guessed.

        When the method is local-file, returned headers will include
        a Date representing the file's last-modified time, a
        Content-Length giving file size, and a Content-Type
        containing a guess at the file's type. See also the
        description of the mimetools module.

        """

    def geturl(self) -> str:
        """Return the real URL of the page.

        In some cases, the HTTP server redirects a client to another
        URL. The urlopen() function handles this transparently, but in
        some cases the caller needs to know which URL the client was
        redirected to. The geturl() method can be used to get at this
        redirected URL.

        """

    def getcode(self) -> int:
        """Return the HTTP status code that was sent with the response,
        or None if the URL is not an HTTP URL.

        """

    def begin(self) -> None: ...

class HTTPConnection:
    auto_open: int  # undocumented
    debuglevel: int
    default_port: int  # undocumented
    response_class: type[HTTPResponse]  # undocumented
    timeout: float | None
    host: str
    port: int
    sock: socket | MaybeNone  # can be `None` if `.connect()` was not called
    def __init__(
        self,
        host: str,
        port: int | None = None,
        timeout: float | None = ...,
        source_address: tuple[str, int] | None = None,
        blocksize: int = 8192,
    ) -> None: ...
    def request(
        self,
        method: str,
        url: str,
        body: _DataType | str | None = None,
        headers: Mapping[str, _HeaderValue] = {},
        *,
        encode_chunked: bool = False,
    ) -> None:
        """Send a complete request to the server."""

    def getresponse(self) -> HTTPResponse:
        """Get the response from the server.

        If the HTTPConnection is in the correct state, returns an
        instance of HTTPResponse or of whatever object is returned by
        the response_class variable.

        If a request has not been sent or if a previous response has
        not be handled, ResponseNotReady is raised.  If the HTTP
        response indicates that the connection should be closed, then
        it will be closed before the response is returned.  When the
        connection is closed, the underlying socket is closed.
        """

    def set_debuglevel(self, level: int) -> None: ...
    if sys.version_info >= (3, 12):
        def get_proxy_response_headers(self) -> HTTPMessage | None:
            """
            Returns a dictionary with the headers of the response
            received from the proxy server to the CONNECT request
            sent to set the tunnel.

            If the CONNECT request was not sent, the method returns None.
            """

    def set_tunnel(self, host: str, port: int | None = None, headers: Mapping[str, str] | None = None) -> None:
        """Set up host and port for HTTP CONNECT tunnelling.

        In a connection that uses HTTP CONNECT tunnelling, the host passed to
        the constructor is used as a proxy server that relays all communication
        to the endpoint passed to `set_tunnel`. This done by sending an HTTP
        CONNECT request to the proxy server when the connection is established.

        This method must be called before the HTTP connection has been
        established.

        The headers argument should be a mapping of extra HTTP headers to send
        with the CONNECT request.

        As HTTP/1.1 is used for HTTP CONNECT tunnelling request, as per the RFC
        (https://tools.ietf.org/html/rfc7231#section-4.3.6), a HTTP Host:
        header must be provided, matching the authority-form of the request
        target provided as the destination for the CONNECT request. If a
        HTTP Host: header is not provided via the headers argument, one
        is generated and transmitted automatically.
        """

    def connect(self) -> None:
        """Connect to the host and port specified in __init__."""

    def close(self) -> None:
        """Close the connection to the HTTP server."""

    def putrequest(self, method: str, url: str, skip_host: bool = False, skip_accept_encoding: bool = False) -> None:
        """Send a request to the server.

        'method' specifies an HTTP request method, e.g. 'GET'.
        'url' specifies the object being requested, e.g. '/index.html'.
        'skip_host' if True does not add automatically a 'Host:' header
        'skip_accept_encoding' if True does not add automatically an
           'Accept-Encoding:' header
        """

    def putheader(self, header: str | bytes, *values: _HeaderValue) -> None:
        """Send a request header line to the server.

        For example: h.putheader('Accept', 'text/html')
        """

    def endheaders(self, message_body: _DataType | None = None, *, encode_chunked: bool = False) -> None:
        """Indicate that the last header line has been sent to the server.

        This method sends the request to the server.  The optional message_body
        argument can be used to pass a message body associated with the
        request.
        """

    def send(self, data: _DataType | str) -> None:
        """Send 'data' to the server.
        ``data`` can be a string object, a bytes object, an array object, a
        file-like object that supports a .read() method, or an iterable object.
        """

class HTTPSConnection(HTTPConnection):
    """This class allows communication via SSL."""

    # Can be `None` if `.connect()` was not called:
    sock: ssl.SSLSocket | MaybeNone
    if sys.version_info >= (3, 12):
        def __init__(
            self,
            host: str,
            port: int | None = None,
            *,
            timeout: float | None = ...,
            source_address: tuple[str, int] | None = None,
            context: ssl.SSLContext | None = None,
            blocksize: int = 8192,
        ) -> None: ...
    else:
        def __init__(
            self,
            host: str,
            port: int | None = None,
            key_file: str | None = None,
            cert_file: str | None = None,
            timeout: float | None = ...,
            source_address: tuple[str, int] | None = None,
            *,
            context: ssl.SSLContext | None = None,
            check_hostname: bool | None = None,
            blocksize: int = 8192,
        ) -> None: ...

class HTTPException(Exception): ...

error = HTTPException

class NotConnected(HTTPException): ...
class InvalidURL(HTTPException): ...

class UnknownProtocol(HTTPException):
    def __init__(self, version: str) -> None: ...

class UnknownTransferEncoding(HTTPException): ...
class UnimplementedFileMode(HTTPException): ...

class IncompleteRead(HTTPException):
    def __init__(self, partial: bytes, expected: int | None = None) -> None: ...
    partial: bytes
    expected: int | None

class ImproperConnectionState(HTTPException): ...
class CannotSendRequest(ImproperConnectionState): ...
class CannotSendHeader(ImproperConnectionState): ...
class ResponseNotReady(ImproperConnectionState): ...

class BadStatusLine(HTTPException):
    def __init__(self, line: str) -> None: ...

class LineTooLong(HTTPException):
    def __init__(self, line_type: str) -> None: ...

class RemoteDisconnected(ConnectionResetError, BadStatusLine): ...
