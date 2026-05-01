"""
An XML-RPC client interface for Python.

The marshalling and response parser code can also be used to
implement XML-RPC servers.

Exported exceptions:

  Error          Base class for client errors
  ProtocolError  Indicates an HTTP protocol error
  ResponseError  Indicates a broken response package
  Fault          Indicates an XML-RPC fault package

Exported classes:

  ServerProxy    Represents a logical connection to an XML-RPC server

  MultiCall      Executor of boxcared xmlrpc requests
  DateTime       dateTime wrapper for an ISO 8601 string or time tuple or
                 localtime integer value to generate a "dateTime.iso8601"
                 XML-RPC value
  Binary         binary data wrapper

  Marshaller     Generate an XML-RPC params chunk from a Python data structure
  Unmarshaller   Unmarshal an XML-RPC response from incoming XML event message
  Transport      Handles an HTTP transaction to an XML-RPC server
  SafeTransport  Handles an HTTPS transaction to an XML-RPC server

Exported constants:

  (none)

Exported functions:

  getparser      Create instance of the fastest available parser & attach
                 to an unmarshalling object
  dumps          Convert an argument tuple or a Fault instance to an XML-RPC
                 request (or response, if the methodresponse option is used).
  loads          Convert an XML-RPC packet to unmarshalled data plus a method
                 name (None if not present).
"""

import gzip
import http.client
import time
from _typeshed import ReadableBuffer, SizedBuffer, SupportsRead, SupportsWrite
from collections.abc import Callable, Iterable, Mapping
from datetime import datetime
from io import BytesIO
from types import TracebackType
from typing import Any, ClassVar, Final, Literal, Protocol, overload, type_check_only
from typing_extensions import Self, TypeAlias

@type_check_only
class _SupportsTimeTuple(Protocol):
    def timetuple(self) -> time.struct_time: ...

_DateTimeComparable: TypeAlias = DateTime | datetime | str | _SupportsTimeTuple
_Marshallable: TypeAlias = (
    bool
    | int
    | float
    | str
    | bytes
    | bytearray
    | None
    | tuple[_Marshallable, ...]
    # Ideally we'd use _Marshallable for list and dict, but invariance makes that impractical
    | list[Any]
    | dict[str, Any]
    | datetime
    | DateTime
    | Binary
)
_XMLDate: TypeAlias = int | datetime | tuple[int, ...] | time.struct_time
_HostType: TypeAlias = tuple[str, dict[str, str]] | str

def escape(s: str) -> str: ...  # undocumented

MAXINT: Final[int]  # undocumented
MININT: Final[int]  # undocumented

PARSE_ERROR: Final[int]  # undocumented
SERVER_ERROR: Final[int]  # undocumented
APPLICATION_ERROR: Final[int]  # undocumented
SYSTEM_ERROR: Final[int]  # undocumented
TRANSPORT_ERROR: Final[int]  # undocumented

NOT_WELLFORMED_ERROR: Final[int]  # undocumented
UNSUPPORTED_ENCODING: Final[int]  # undocumented
INVALID_ENCODING_CHAR: Final[int]  # undocumented
INVALID_XMLRPC: Final[int]  # undocumented
METHOD_NOT_FOUND: Final[int]  # undocumented
INVALID_METHOD_PARAMS: Final[int]  # undocumented
INTERNAL_ERROR: Final[int]  # undocumented

class Error(Exception):
    """Base class for client errors."""

class ProtocolError(Error):
    """Indicates an HTTP protocol error."""

    url: str
    errcode: int
    errmsg: str
    headers: dict[str, str]
    def __init__(self, url: str, errcode: int, errmsg: str, headers: dict[str, str]) -> None: ...

class ResponseError(Error):
    """Indicates a broken response package."""

class Fault(Error):
    """Indicates an XML-RPC fault package."""

    faultCode: int
    faultString: str
    def __init__(self, faultCode: int, faultString: str, **extra: Any) -> None: ...

boolean = bool
Boolean = bool

def _iso8601_format(value: datetime) -> str: ...  # undocumented
def _strftime(value: _XMLDate) -> str: ...  # undocumented

class DateTime:
    """DateTime wrapper for an ISO 8601 string or time tuple or
    localtime integer value to generate 'dateTime.iso8601' XML-RPC
    value.
    """

    value: str  # undocumented
    def __init__(self, value: int | str | datetime | time.struct_time | tuple[int, ...] = 0) -> None: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __lt__(self, other: _DateTimeComparable) -> bool: ...
    def __le__(self, other: _DateTimeComparable) -> bool: ...
    def __gt__(self, other: _DateTimeComparable) -> bool: ...
    def __ge__(self, other: _DateTimeComparable) -> bool: ...
    def __eq__(self, other: _DateTimeComparable) -> bool: ...  # type: ignore[override]
    def make_comparable(self, other: _DateTimeComparable) -> tuple[str, str]: ...  # undocumented
    def timetuple(self) -> time.struct_time: ...  # undocumented
    def decode(self, data: Any) -> None: ...
    def encode(self, out: SupportsWrite[str]) -> None: ...

def _datetime(data: Any) -> DateTime: ...  # undocumented
def _datetime_type(data: str) -> datetime: ...  # undocumented

class Binary:
    """Wrapper for binary data."""

    data: bytes
    def __init__(self, data: bytes | bytearray | None = None) -> None: ...
    def decode(self, data: ReadableBuffer) -> None: ...
    def encode(self, out: SupportsWrite[str]) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

def _binary(data: ReadableBuffer) -> Binary: ...  # undocumented

WRAPPERS: Final[tuple[type[DateTime], type[Binary]]]  # undocumented

class ExpatParser:  # undocumented
    def __init__(self, target: Unmarshaller) -> None: ...
    def feed(self, data: str | ReadableBuffer) -> None: ...
    def close(self) -> None: ...

_WriteCallback: TypeAlias = Callable[[str], object]

class Marshaller:
    """Generate an XML-RPC params chunk from a Python data structure.

    Create a Marshaller instance for each set of parameters, and use
    the "dumps" method to convert your data (represented as a tuple)
    to an XML-RPC params chunk.  To write a fault response, pass a
    Fault instance instead.  You may prefer to use the "dumps" module
    function for this purpose.
    """

    dispatch: dict[type[_Marshallable] | Literal["_arbitrary_instance"], Callable[[Marshaller, Any, _WriteCallback], None]]
    memo: dict[Any, None]
    data: None
    encoding: str | None
    allow_none: bool
    def __init__(self, encoding: str | None = None, allow_none: bool = False) -> None: ...
    def dumps(self, values: Fault | Iterable[_Marshallable]) -> str: ...
    def __dump(self, value: _Marshallable, write: _WriteCallback) -> None: ...  # undocumented
    def dump_nil(self, value: None, write: _WriteCallback) -> None: ...
    def dump_bool(self, value: bool, write: _WriteCallback) -> None: ...
    def dump_long(self, value: int, write: _WriteCallback) -> None: ...
    def dump_int(self, value: int, write: _WriteCallback) -> None: ...
    def dump_double(self, value: float, write: _WriteCallback) -> None: ...
    def dump_unicode(self, value: str, write: _WriteCallback, escape: Callable[[str], str] = ...) -> None: ...
    def dump_bytes(self, value: ReadableBuffer, write: _WriteCallback) -> None: ...
    def dump_array(self, value: Iterable[_Marshallable], write: _WriteCallback) -> None: ...
    def dump_struct(
        self, value: Mapping[str, _Marshallable], write: _WriteCallback, escape: Callable[[str], str] = ...
    ) -> None: ...
    def dump_datetime(self, value: _XMLDate, write: _WriteCallback) -> None: ...
    def dump_instance(self, value: object, write: _WriteCallback) -> None: ...

class Unmarshaller:
    """Unmarshal an XML-RPC response, based on incoming XML event
    messages (start, data, end).  Call close() to get the resulting
    data structure.

    Note that this reader is fairly tolerant, and gladly accepts bogus
    XML-RPC data without complaining (but not bogus XML).
    """

    dispatch: dict[str, Callable[[Unmarshaller, str], None]]

    _type: str | None
    _stack: list[_Marshallable]
    _marks: list[int]
    _data: list[str]
    _value: bool
    _methodname: str | None
    _encoding: str
    append: Callable[[Any], None]
    _use_datetime: bool
    _use_builtin_types: bool
    def __init__(self, use_datetime: bool = False, use_builtin_types: bool = False) -> None: ...
    def close(self) -> tuple[_Marshallable, ...]: ...
    def getmethodname(self) -> str | None: ...
    def xml(self, encoding: str, standalone: Any) -> None: ...  # Standalone is ignored
    def start(self, tag: str, attrs: dict[str, str]) -> None: ...
    def data(self, text: str) -> None: ...
    def end(self, tag: str) -> None: ...
    def end_dispatch(self, tag: str, data: str) -> None: ...
    def end_nil(self, data: str) -> None: ...
    def end_boolean(self, data: str) -> None: ...
    def end_int(self, data: str) -> None: ...
    def end_double(self, data: str) -> None: ...
    def end_bigdecimal(self, data: str) -> None: ...
    def end_string(self, data: str) -> None: ...
    def end_array(self, data: str) -> None: ...
    def end_struct(self, data: str) -> None: ...
    def end_base64(self, data: str) -> None: ...
    def end_dateTime(self, data: str) -> None: ...
    def end_value(self, data: str) -> None: ...
    def end_params(self, data: str) -> None: ...
    def end_fault(self, data: str) -> None: ...
    def end_methodName(self, data: str) -> None: ...

class _MultiCallMethod:  # undocumented
    __call_list: list[tuple[str, tuple[_Marshallable, ...]]]
    __name: str
    def __init__(self, call_list: list[tuple[str, _Marshallable]], name: str) -> None: ...
    def __getattr__(self, name: str) -> _MultiCallMethod: ...
    def __call__(self, *args: _Marshallable) -> None: ...

class MultiCallIterator:  # undocumented
    """Iterates over the results of a multicall. Exceptions are
    raised in response to xmlrpc faults.
    """

    results: list[list[_Marshallable]]
    def __init__(self, results: list[list[_Marshallable]]) -> None: ...
    def __getitem__(self, i: int) -> _Marshallable: ...

class MultiCall:
    """server -> an object used to boxcar method calls

    server should be a ServerProxy object.

    Methods can be added to the MultiCall using normal
    method call syntax e.g.:

    multicall = MultiCall(server_proxy)
    multicall.add(2,3)
    multicall.get_address("Guido")

    To execute the multicall, call the MultiCall object e.g.:

    add_result, address = multicall()
    """

    __server: ServerProxy
    __call_list: list[tuple[str, tuple[_Marshallable, ...]]]
    def __init__(self, server: ServerProxy) -> None: ...
    def __getattr__(self, name: str) -> _MultiCallMethod: ...
    def __call__(self) -> MultiCallIterator: ...

# A little white lie
FastMarshaller: Marshaller | None
FastParser: ExpatParser | None
FastUnmarshaller: Unmarshaller | None

def getparser(use_datetime: bool = False, use_builtin_types: bool = False) -> tuple[ExpatParser, Unmarshaller]:
    """getparser() -> parser, unmarshaller

    Create an instance of the fastest available parser, and attach it
    to an unmarshalling object.  Return both objects.
    """

def dumps(
    params: Fault | tuple[_Marshallable, ...],
    methodname: str | None = None,
    methodresponse: bool | None = None,
    encoding: str | None = None,
    allow_none: bool = False,
) -> str:
    """data [,options] -> marshalled data

    Convert an argument tuple or a Fault instance to an XML-RPC
    request (or response, if the methodresponse option is used).

    In addition to the data object, the following options can be given
    as keyword arguments:

        methodname: the method name for a methodCall packet

        methodresponse: true to create a methodResponse packet.
        If this option is used with a tuple, the tuple must be
        a singleton (i.e. it can contain only one element).

        encoding: the packet encoding (default is UTF-8)

    All byte strings in the data structure are assumed to use the
    packet encoding.  Unicode strings are automatically converted,
    where necessary.
    """

def loads(
    data: str | ReadableBuffer, use_datetime: bool = False, use_builtin_types: bool = False
) -> tuple[tuple[_Marshallable, ...], str | None]:
    """data -> unmarshalled data, method name

    Convert an XML-RPC packet to unmarshalled data plus a method
    name (None if not present).

    If the XML-RPC packet represents a fault condition, this function
    raises a Fault exception.
    """

def gzip_encode(data: ReadableBuffer) -> bytes:  # undocumented
    """data -> gzip encoded data

    Encode data using the gzip content encoding as described in RFC 1952
    """

def gzip_decode(data: ReadableBuffer, max_decode: int = 20971520) -> bytes:  # undocumented
    """gzip encoded data -> unencoded data

    Decode data using the gzip content encoding as described in RFC 1952
    """

class GzipDecodedResponse(gzip.GzipFile):  # undocumented
    """a file-like object to decode a response encoded with the gzip
    method, as described in RFC 1952.
    """

    io: BytesIO
    def __init__(self, response: SupportsRead[ReadableBuffer]) -> None: ...

class _Method:  # undocumented
    __send: Callable[[str, tuple[_Marshallable, ...]], _Marshallable]
    __name: str
    def __init__(self, send: Callable[[str, tuple[_Marshallable, ...]], _Marshallable], name: str) -> None: ...
    def __getattr__(self, name: str) -> _Method: ...
    def __call__(self, *args: _Marshallable) -> _Marshallable: ...

class Transport:
    """Handles an HTTP transaction to an XML-RPC server."""

    user_agent: str
    accept_gzip_encoding: bool
    encode_threshold: int | None

    _use_datetime: bool
    _use_builtin_types: bool
    _connection: tuple[_HostType | None, http.client.HTTPConnection | None]
    _headers: list[tuple[str, str]]
    _extra_headers: list[tuple[str, str]]

    def __init__(
        self, use_datetime: bool = False, use_builtin_types: bool = False, *, headers: Iterable[tuple[str, str]] = ()
    ) -> None: ...
    def request(
        self, host: _HostType, handler: str, request_body: SizedBuffer, verbose: bool = False
    ) -> tuple[_Marshallable, ...]: ...
    def single_request(
        self, host: _HostType, handler: str, request_body: SizedBuffer, verbose: bool = False
    ) -> tuple[_Marshallable, ...]: ...
    def getparser(self) -> tuple[ExpatParser, Unmarshaller]: ...
    def get_host_info(self, host: _HostType) -> tuple[str, list[tuple[str, str]], dict[str, str]]: ...
    def make_connection(self, host: _HostType) -> http.client.HTTPConnection: ...
    def close(self) -> None: ...
    def send_request(
        self, host: _HostType, handler: str, request_body: SizedBuffer, debug: bool
    ) -> http.client.HTTPConnection: ...
    def send_headers(self, connection: http.client.HTTPConnection, headers: list[tuple[str, str]]) -> None: ...
    def send_content(self, connection: http.client.HTTPConnection, request_body: SizedBuffer) -> None: ...
    def parse_response(self, response: http.client.HTTPResponse) -> tuple[_Marshallable, ...]: ...

class SafeTransport(Transport):
    """Handles an HTTPS transaction to an XML-RPC server."""

    def __init__(
        self,
        use_datetime: bool = False,
        use_builtin_types: bool = False,
        *,
        headers: Iterable[tuple[str, str]] = (),
        context: Any | None = None,
    ) -> None: ...
    def make_connection(self, host: _HostType) -> http.client.HTTPSConnection: ...

class ServerProxy:
    """uri [,options] -> a logical connection to an XML-RPC server

    uri is the connection point on the server, given as
    scheme://host/target.

    The standard implementation always supports the "http" scheme.  If
    SSL socket support is available (Python 2.0), it also supports
    "https".

    If the target part and the slash preceding it are both omitted,
    "/RPC2" is assumed.

    The following options can be given as keyword arguments:

        transport: a transport factory
        encoding: the request encoding (default is UTF-8)

    All 8-bit strings passed to the server proxy are assumed to use
    the given encoding.
    """

    __host: str
    __handler: str
    __transport: Transport
    __encoding: str
    __verbose: bool
    __allow_none: bool

    def __init__(
        self,
        uri: str,
        transport: Transport | None = None,
        encoding: str | None = None,
        verbose: bool = False,
        allow_none: bool = False,
        use_datetime: bool = False,
        use_builtin_types: bool = False,
        *,
        headers: Iterable[tuple[str, str]] = (),
        context: Any | None = None,
    ) -> None: ...
    def __getattr__(self, name: str) -> _Method: ...
    @overload
    def __call__(self, attr: Literal["close"]) -> Callable[[], None]:
        """A workaround to get special attributes on the ServerProxy
        without interfering with the magic __getattr__
        """

    @overload
    def __call__(self, attr: Literal["transport"]) -> Transport: ...
    @overload
    def __call__(self, attr: str) -> Callable[[], None] | Transport: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    def __close(self) -> None: ...  # undocumented
    def __request(self, methodname: str, params: tuple[_Marshallable, ...]) -> tuple[_Marshallable, ...]: ...  # undocumented

Server = ServerProxy
