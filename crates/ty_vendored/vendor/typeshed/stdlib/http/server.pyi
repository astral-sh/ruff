import _socket
import email.message
import io
import socketserver
import sys
from _ssl import _PasswordType
from _typeshed import ReadableBuffer, StrOrBytesPath, StrPath, SupportsRead, SupportsWrite
from collections.abc import Callable, Iterable, Mapping, Sequence
from ssl import Purpose, SSLContext
from typing import Any, AnyStr, BinaryIO, ClassVar, Protocol, type_check_only
from typing_extensions import Self, deprecated

if sys.version_info >= (3, 14):
    __all__ = [
        "HTTPServer",
        "ThreadingHTTPServer",
        "HTTPSServer",
        "ThreadingHTTPSServer",
        "BaseHTTPRequestHandler",
        "SimpleHTTPRequestHandler",
        "CGIHTTPRequestHandler",
    ]
else:
    __all__ = ["HTTPServer", "ThreadingHTTPServer", "BaseHTTPRequestHandler", "SimpleHTTPRequestHandler", "CGIHTTPRequestHandler"]

class HTTPServer(socketserver.TCPServer):
    server_name: str
    server_port: int

class ThreadingHTTPServer(socketserver.ThreadingMixIn, HTTPServer): ...

if sys.version_info >= (3, 14):
    @type_check_only
    class _SSLModule(Protocol):
        @staticmethod
        def create_default_context(
            purpose: Purpose = ...,
            *,
            cafile: StrOrBytesPath | None = None,
            capath: StrOrBytesPath | None = None,
            cadata: str | ReadableBuffer | None = None,
        ) -> SSLContext: ...

    class HTTPSServer(HTTPServer):
        ssl: _SSLModule
        certfile: StrOrBytesPath
        keyfile: StrOrBytesPath | None
        password: _PasswordType | None
        alpn_protocols: Iterable[str]
        def __init__(
            self,
            server_address: socketserver._AfInetAddress,
            RequestHandlerClass: Callable[[Any, _socket._RetAddress, Self], socketserver.BaseRequestHandler],
            bind_and_activate: bool = True,
            *,
            certfile: StrOrBytesPath,
            keyfile: StrOrBytesPath | None = None,
            password: _PasswordType | None = None,
            alpn_protocols: Iterable[str] | None = None,
        ) -> None: ...
        def server_activate(self) -> None: ...

    class ThreadingHTTPSServer(socketserver.ThreadingMixIn, HTTPSServer): ...

class BaseHTTPRequestHandler(socketserver.StreamRequestHandler):
    client_address: tuple[str, int]
    close_connection: bool
    requestline: str
    command: str
    path: str
    request_version: str
    headers: email.message.Message
    server_version: str
    sys_version: str
    error_message_format: str
    error_content_type: str
    protocol_version: str
    MessageClass: type
    responses: Mapping[int, tuple[str, str]]
    default_request_version: str  # undocumented
    weekdayname: ClassVar[Sequence[str]]  # undocumented
    monthname: ClassVar[Sequence[str | None]]  # undocumented
    def handle_one_request(self) -> None: ...
    def handle_expect_100(self) -> bool: ...
    def send_error(self, code: int, message: str | None = None, explain: str | None = None) -> None: ...
    def send_response(self, code: int, message: str | None = None) -> None: ...
    def send_header(self, keyword: str, value: str) -> None: ...
    def send_response_only(self, code: int, message: str | None = None) -> None: ...
    def end_headers(self) -> None: ...
    def flush_headers(self) -> None: ...
    def log_request(self, code: int | str = "-", size: int | str = "-") -> None: ...
    def log_error(self, format: str, *args: Any) -> None: ...
    def log_message(self, format: str, *args: Any) -> None: ...
    def version_string(self) -> str: ...
    def date_time_string(self, timestamp: float | None = None) -> str: ...
    def log_date_time_string(self) -> str: ...
    def address_string(self) -> str: ...
    def parse_request(self) -> bool: ...  # undocumented

class SimpleHTTPRequestHandler(BaseHTTPRequestHandler):
    extensions_map: dict[str, str]
    if sys.version_info >= (3, 12):
        index_pages: ClassVar[tuple[str, ...]]
    directory: str
    def __init__(
        self,
        request: socketserver._RequestType,
        client_address: _socket._RetAddress,
        server: socketserver.BaseServer,
        *,
        directory: StrPath | None = None,
    ) -> None: ...
    def do_GET(self) -> None: ...
    def do_HEAD(self) -> None: ...
    def send_head(self) -> io.BytesIO | BinaryIO | None: ...  # undocumented
    def list_directory(self, path: StrPath) -> io.BytesIO | None: ...  # undocumented
    def translate_path(self, path: str) -> str: ...  # undocumented
    def copyfile(self, source: SupportsRead[AnyStr], outputfile: SupportsWrite[AnyStr]) -> None: ...  # undocumented
    def guess_type(self, path: StrPath) -> str: ...  # undocumented

def executable(path: StrPath) -> bool: ...  # undocumented

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13; will be removed in Python 3.15.")
    class CGIHTTPRequestHandler(SimpleHTTPRequestHandler):
        cgi_directories: list[str]
        have_fork: bool  # undocumented
        def do_POST(self) -> None: ...
        def is_cgi(self) -> bool: ...  # undocumented
        def is_executable(self, path: StrPath) -> bool: ...  # undocumented
        def is_python(self, path: StrPath) -> bool: ...  # undocumented
        def run_cgi(self) -> None: ...  # undocumented

else:
    class CGIHTTPRequestHandler(SimpleHTTPRequestHandler):
        cgi_directories: list[str]
        have_fork: bool  # undocumented
        def do_POST(self) -> None: ...
        def is_cgi(self) -> bool: ...  # undocumented
        def is_executable(self, path: StrPath) -> bool: ...  # undocumented
        def is_python(self, path: StrPath) -> bool: ...  # undocumented
        def run_cgi(self) -> None: ...  # undocumented
