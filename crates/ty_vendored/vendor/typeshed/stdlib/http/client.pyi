import email.message
import io
import ssl
import sys
import types
from _typeshed import MaybeNone, ReadableBuffer, SupportsRead, SupportsReadline, WriteableBuffer
from collections.abc import Callable, Iterable, Iterator, Mapping
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
_MessageT = TypeVar("_MessageT", bound=email.message.Message)
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
