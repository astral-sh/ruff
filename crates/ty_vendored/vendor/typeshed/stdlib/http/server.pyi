"""HTTP server classes.

Note: BaseHTTPRequestHandler doesn't implement any HTTP request; see
SimpleHTTPRequestHandler for simple implementations of GET, HEAD and POST,
and (deprecated) CGIHTTPRequestHandler for CGI scripts.

It does, however, optionally implement HTTP/1.1 persistent connections.

Notes on CGIHTTPRequestHandler
------------------------------

This class is deprecated. It implements GET and POST requests to cgi-bin scripts.

If the os.fork() function is not present (Windows), subprocess.Popen() is used,
with slightly altered but never documented semantics.  Use from a threaded
process is likely to trigger a warning at os.fork() time.

In all cases, the implementation is intentionally naive -- all
requests are executed synchronously.

SECURITY WARNING: DON'T USE THIS CODE UNLESS YOU ARE INSIDE A FIREWALL
-- it may execute arbitrary Python code or external programs.

Note that status code 200 is sent prior to execution of a CGI script, so
scripts cannot send other status codes such as 302 (redirect).

XXX To do:

- log requests even later (to capture byte count)
- log user-agent header and other interesting goodies
- send error log to separate file
"""

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
        def server_activate(self) -> None:
            """Wrap the socket in SSLSocket."""

    class ThreadingHTTPSServer(socketserver.ThreadingMixIn, HTTPSServer): ...

class BaseHTTPRequestHandler(socketserver.StreamRequestHandler):
    """HTTP request handler base class.

    The following explanation of HTTP serves to guide you through the
    code as well as to expose any misunderstandings I may have about
    HTTP (so you don't need to read the code to figure out I'm wrong
    :-).

    HTTP (HyperText Transfer Protocol) is an extensible protocol on
    top of a reliable stream transport (e.g. TCP/IP).  The protocol
    recognizes three parts to a request:

    1. One line identifying the request type and path
    2. An optional set of RFC-822-style headers
    3. An optional data part

    The headers and data are separated by a blank line.

    The first line of the request has the form

    <command> <path> <version>

    where <command> is a (case-sensitive) keyword such as GET or POST,
    <path> is a string containing path information for the request,
    and <version> should be the string "HTTP/1.0" or "HTTP/1.1".
    <path> is encoded using the URL encoding scheme (using %xx to signify
    the ASCII character with hex code xx).

    The specification specifies that lines are separated by CRLF but
    for compatibility with the widest range of clients recommends
    servers also handle LF.  Similarly, whitespace in the request line
    is treated sensibly (allowing multiple spaces between components
    and allowing trailing whitespace).

    Similarly, for output, lines ought to be separated by CRLF pairs
    but most clients grok LF characters just fine.

    If the first line of the request has the form

    <command> <path>

    (i.e. <version> is left out) then this is assumed to be an HTTP
    0.9 request; this form has no optional headers and data part and
    the reply consists of just the data.

    The reply form of the HTTP 1.x protocol again has three parts:

    1. One line giving the response code
    2. An optional set of RFC-822-style headers
    3. The data

    Again, the headers and data are separated by a blank line.

    The response code line has the form

    <version> <responsecode> <responsestring>

    where <version> is the protocol version ("HTTP/1.0" or "HTTP/1.1"),
    <responsecode> is a 3-digit response code indicating success or
    failure of the request, and <responsestring> is an optional
    human-readable string explaining what the response code means.

    This server parses the request and the headers, and then calls a
    function specific to the request type (<command>).  Specifically,
    a request SPAM will be handled by a method do_SPAM().  If no
    such method exists the server sends an error response to the
    client.  If it exists, it is called with no arguments:

    do_SPAM()

    Note that the request name is case sensitive (i.e. SPAM and spam
    are different requests).

    The various request details are stored in instance variables:

    - client_address is the client IP address in the form (host,
    port);

    - command, path and version are the broken-down request line;

    - headers is an instance of email.message.Message (or a derived
    class) containing the header information;

    - rfile is a file object open for reading positioned at the
    start of the optional input data part;

    - wfile is a file object open for writing.

    IT IS IMPORTANT TO ADHERE TO THE PROTOCOL FOR WRITING!

    The first thing to be written must be the response line.  Then
    follow 0 or more header lines, then a blank line, and then the
    actual data (if any).  The meaning of the header lines depends on
    the command executed by the server; in most cases, when data is
    returned, there should be at least one header line of the form

    Content-type: <type>/<subtype>

    where <type> and <subtype> should be registered MIME types,
    e.g. "text/html" or "text/plain".

    """

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
    def handle_one_request(self) -> None:
        """Handle a single HTTP request.

        You normally don't need to override this method; see the class
        __doc__ string for information on how to handle specific HTTP
        commands such as GET and POST.

        """

    def handle_expect_100(self) -> bool:
        """Decide what to do with an "Expect: 100-continue" header.

        If the client is expecting a 100 Continue response, we must
        respond with either a 100 Continue or a final response before
        waiting for the request body. The default is to always respond
        with a 100 Continue. You can behave differently (for example,
        reject unauthorized requests) by overriding this method.

        This method should either return True (possibly after sending
        a 100 Continue response) or send an error response and return
        False.

        """

    def send_error(self, code: int, message: str | None = None, explain: str | None = None) -> None:
        """Send and log an error reply.

        Arguments are
        * code:    an HTTP error code
                   3 digits
        * message: a simple optional 1 line reason phrase.
                   *( HTAB / SP / VCHAR / %x80-FF )
                   defaults to short entry matching the response code
        * explain: a detailed message defaults to the long entry
                   matching the response code.

        This sends an error response (so it must be called before any
        output has been generated), logs the error, and finally sends
        a piece of HTML explaining the error to the user.

        """

    def send_response(self, code: int, message: str | None = None) -> None:
        """Add the response header to the headers buffer and log the
        response code.

        Also send two standard headers with the server software
        version and the current date.

        """

    def send_header(self, keyword: str, value: str) -> None:
        """Send a MIME header to the headers buffer."""

    def send_response_only(self, code: int, message: str | None = None) -> None:
        """Send the response header only."""

    def end_headers(self) -> None:
        """Send the blank line ending the MIME headers."""

    def flush_headers(self) -> None: ...
    def log_request(self, code: int | str = "-", size: int | str = "-") -> None:
        """Log an accepted request.

        This is called by send_response().

        """

    def log_error(self, format: str, *args: Any) -> None:
        """Log an error.

        This is called when a request cannot be fulfilled.  By
        default it passes the message on to log_message().

        Arguments are the same as for log_message().

        XXX This should go to the separate error log.

        """

    def log_message(self, format: str, *args: Any) -> None:
        """Log an arbitrary message.

        This is used by all other logging functions.  Override
        it if you have specific logging wishes.

        The first argument, FORMAT, is a format string for the
        message to be logged.  If the format string contains
        any % escapes requiring parameters, they should be
        specified as subsequent arguments (it's just like
        printf!).

        The client ip and current date/time are prefixed to
        every message.

        Unicode control characters are replaced with escaped hex
        before writing the output to stderr.

        """

    def version_string(self) -> str:
        """Return the server software version string."""

    def date_time_string(self, timestamp: float | None = None) -> str:
        """Return the current date and time formatted for a message header."""

    def log_date_time_string(self) -> str:
        """Return the current time formatted for logging."""

    def address_string(self) -> str:
        """Return the client address."""

    def parse_request(self) -> bool:  # undocumented
        """Parse a request (internal).

        The request should be stored in self.raw_requestline; the results
        are in self.command, self.path, self.request_version and
        self.headers.

        Return True for success, False for failure; on failure, any relevant
        error response has already been sent back.

        """

class SimpleHTTPRequestHandler(BaseHTTPRequestHandler):
    """Simple HTTP request handler with GET and HEAD commands.

    This serves files from the current directory and any of its
    subdirectories.  The MIME type for files is determined by
    calling the .guess_type() method.

    The GET and HEAD requests are identical except that the HEAD
    request omits the actual contents of the file.

    """

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
    def do_GET(self) -> None:
        """Serve a GET request."""

    def do_HEAD(self) -> None:
        """Serve a HEAD request."""

    def send_head(self) -> io.BytesIO | BinaryIO | None:  # undocumented
        """Common code for GET and HEAD commands.

        This sends the response code and MIME headers.

        Return value is either a file object (which has to be copied
        to the outputfile by the caller unless the command was HEAD,
        and must be closed by the caller under all circumstances), or
        None, in which case the caller has nothing further to do.

        """

    def list_directory(self, path: StrPath) -> io.BytesIO | None:  # undocumented
        """Helper to produce a directory listing (absent index.html).

        Return value is either a file object, or None (indicating an
        error).  In either case, the headers are sent, making the
        interface the same as for send_head().

        """

    def translate_path(self, path: str) -> str:  # undocumented
        """Translate a /-separated PATH to the local filename syntax.

        Components that mean special things to the local file system
        (e.g. drive or directory names) are ignored.  (XXX They should
        probably be diagnosed.)

        """

    def copyfile(self, source: SupportsRead[AnyStr], outputfile: SupportsWrite[AnyStr]) -> None:  # undocumented
        """Copy all data between two file objects.

        The SOURCE argument is a file object open for reading
        (or anything with a read() method) and the DESTINATION
        argument is a file object open for writing (or
        anything with a write() method).

        The only reason for overriding this would be to change
        the block size or perhaps to replace newlines by CRLF
        -- note however that this the default server uses this
        to copy binary data as well.

        """

    def guess_type(self, path: StrPath) -> str:  # undocumented
        """Guess the type of a file.

        Argument is a PATH (a filename).

        Return value is a string of the form type/subtype,
        usable for a MIME Content-type header.

        The default implementation looks the file's extension
        up in the table self.extensions_map, using application/octet-stream
        as a default; however it would be permissible (if
        slow) to look inside the data to make a better guess.

        """

def executable(path: StrPath) -> bool:  # undocumented
    """Test for executable file."""

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13; will be removed in Python 3.15.")
    class CGIHTTPRequestHandler(SimpleHTTPRequestHandler):
        """Complete HTTP server with GET, HEAD and POST commands.

        GET and HEAD also support running CGI scripts.

        The POST command is *only* implemented for CGI scripts.

        """

        cgi_directories: list[str]
        have_fork: bool  # undocumented
        def do_POST(self) -> None:
            """Serve a POST request.

            This is only implemented for CGI scripts.

            """

        def is_cgi(self) -> bool:  # undocumented
            """Test whether self.path corresponds to a CGI script.

            Returns True and updates the cgi_info attribute to the tuple
            (dir, rest) if self.path requires running a CGI script.
            Returns False otherwise.

            If any exception is raised, the caller should assume that
            self.path was rejected as invalid and act accordingly.

            The default implementation tests whether the normalized url
            path begins with one of the strings in self.cgi_directories
            (and the next character is a '/' or the end of the string).

            """

        def is_executable(self, path: StrPath) -> bool:  # undocumented
            """Test whether argument path is an executable file."""

        def is_python(self, path: StrPath) -> bool:  # undocumented
            """Test whether argument path is a Python script."""

        def run_cgi(self) -> None:  # undocumented
            """Execute a CGI script."""

else:
    class CGIHTTPRequestHandler(SimpleHTTPRequestHandler):
        """Complete HTTP server with GET, HEAD and POST commands.

        GET and HEAD also support running CGI scripts.

        The POST command is *only* implemented for CGI scripts.

        """

        cgi_directories: list[str]
        have_fork: bool  # undocumented
        def do_POST(self) -> None:
            """Serve a POST request.

            This is only implemented for CGI scripts.

            """

        def is_cgi(self) -> bool:  # undocumented
            """Test whether self.path corresponds to a CGI script.

            Returns True and updates the cgi_info attribute to the tuple
            (dir, rest) if self.path requires running a CGI script.
            Returns False otherwise.

            If any exception is raised, the caller should assume that
            self.path was rejected as invalid and act accordingly.

            The default implementation tests whether the normalized url
            path begins with one of the strings in self.cgi_directories
            (and the next character is a '/' or the end of the string).

            """

        def is_executable(self, path: StrPath) -> bool:  # undocumented
            """Test whether argument path is an executable file."""

        def is_python(self, path: StrPath) -> bool:  # undocumented
            """Test whether argument path is a Python script."""

        def run_cgi(self) -> None:  # undocumented
            """Execute a CGI script."""
