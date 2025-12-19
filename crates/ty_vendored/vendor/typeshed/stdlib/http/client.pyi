import email.message
import io
import ssl
import sys
import types
from _typeshed import MaybeNone, ReadableBuffer, SupportsRead, SupportsReadline, WriteableBuffer
from collections.abc import Callable, Iterable, Iterator, Mapping
from email._policybase import _MessageT
from socket import socket
from typing import BinaryIO, Final, TypeVar, overload
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

HTTP_PORT: Final = 80
HTTPS_PORT: Final = 443

# Keep these global constants in sync with http.HTTPStatus (http/__init__.pyi).
# They are present for backward compatibility reasons.
CONTINUE: Final = 100
SWITCHING_PROTOCOLS: Final = 101
PROCESSING: Final = 102
EARLY_HINTS: Final = 103

OK: Final = 200
CREATED: Final = 201
ACCEPTED: Final = 202
NON_AUTHORITATIVE_INFORMATION: Final = 203
NO_CONTENT: Final = 204
RESET_CONTENT: Final = 205
PARTIAL_CONTENT: Final = 206
MULTI_STATUS: Final = 207
ALREADY_REPORTED: Final = 208
IM_USED: Final = 226

MULTIPLE_CHOICES: Final = 300
MOVED_PERMANENTLY: Final = 301
FOUND: Final = 302
SEE_OTHER: Final = 303
NOT_MODIFIED: Final = 304
USE_PROXY: Final = 305
TEMPORARY_REDIRECT: Final = 307
PERMANENT_REDIRECT: Final = 308

BAD_REQUEST: Final = 400
UNAUTHORIZED: Final = 401
PAYMENT_REQUIRED: Final = 402
FORBIDDEN: Final = 403
NOT_FOUND: Final = 404
METHOD_NOT_ALLOWED: Final = 405
NOT_ACCEPTABLE: Final = 406
PROXY_AUTHENTICATION_REQUIRED: Final = 407
REQUEST_TIMEOUT: Final = 408
CONFLICT: Final = 409
GONE: Final = 410
LENGTH_REQUIRED: Final = 411
PRECONDITION_FAILED: Final = 412
if sys.version_info >= (3, 13):
    CONTENT_TOO_LARGE: Final = 413
REQUEST_ENTITY_TOO_LARGE: Final = 413
if sys.version_info >= (3, 13):
    URI_TOO_LONG: Final = 414
REQUEST_URI_TOO_LONG: Final = 414
UNSUPPORTED_MEDIA_TYPE: Final = 415
if sys.version_info >= (3, 13):
    RANGE_NOT_SATISFIABLE: Final = 416
REQUESTED_RANGE_NOT_SATISFIABLE: Final = 416
EXPECTATION_FAILED: Final = 417
IM_A_TEAPOT: Final = 418
MISDIRECTED_REQUEST: Final = 421
if sys.version_info >= (3, 13):
    UNPROCESSABLE_CONTENT: Final = 422
UNPROCESSABLE_ENTITY: Final = 422
LOCKED: Final = 423
FAILED_DEPENDENCY: Final = 424
TOO_EARLY: Final = 425
UPGRADE_REQUIRED: Final = 426
PRECONDITION_REQUIRED: Final = 428
TOO_MANY_REQUESTS: Final = 429
REQUEST_HEADER_FIELDS_TOO_LARGE: Final = 431
UNAVAILABLE_FOR_LEGAL_REASONS: Final = 451

INTERNAL_SERVER_ERROR: Final = 500
NOT_IMPLEMENTED: Final = 501
BAD_GATEWAY: Final = 502
SERVICE_UNAVAILABLE: Final = 503
GATEWAY_TIMEOUT: Final = 504
HTTP_VERSION_NOT_SUPPORTED: Final = 505
VARIANT_ALSO_NEGOTIATES: Final = 506
INSUFFICIENT_STORAGE: Final = 507
LOOP_DETECTED: Final = 508
NOT_EXTENDED: Final = 510
NETWORK_AUTHENTICATION_REQUIRED: Final = 511

responses: dict[int, str]

class HTTPMessage(email.message.Message[str, str]):
    def getallmatchingheaders(self, name: str) -> list[str]: ...  # undocumented

@overload
def parse_headers(fp: SupportsReadline[bytes], _class: Callable[[], _MessageT]) -> _MessageT: ...
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
    def read(self, amt: int | None = None) -> bytes: ...
    def read1(self, n: int = -1) -> bytes: ...
    def readinto(self, b: WriteableBuffer) -> int: ...
    def readline(self, limit: int = -1) -> bytes: ...  # type: ignore[override]
    @overload
    def getheader(self, name: str) -> str | None: ...
    @overload
    def getheader(self, name: str, default: _T) -> str | _T: ...
    def getheaders(self) -> list[tuple[str, str]]: ...
    def isclosed(self) -> bool: ...
    def __iter__(self) -> Iterator[bytes]: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
    ) -> None: ...
    def info(self) -> email.message.Message: ...
    def geturl(self) -> str: ...
    def getcode(self) -> int: ...
    def begin(self) -> None: ...

class HTTPConnection:
    blocksize: int
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
    ) -> None: ...
    def getresponse(self) -> HTTPResponse: ...
    def set_debuglevel(self, level: int) -> None: ...
    if sys.version_info >= (3, 12):
        def get_proxy_response_headers(self) -> HTTPMessage | None: ...

    def set_tunnel(self, host: str, port: int | None = None, headers: Mapping[str, str] | None = None) -> None: ...
    def connect(self) -> None: ...
    def close(self) -> None: ...
    def putrequest(self, method: str, url: str, skip_host: bool = False, skip_accept_encoding: bool = False) -> None: ...
    def putheader(self, header: str | bytes, *values: _HeaderValue) -> None: ...
    def endheaders(self, message_body: _DataType | None = None, *, encode_chunked: bool = False) -> None: ...
    def send(self, data: _DataType | str) -> None: ...

class HTTPSConnection(HTTPConnection):
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
