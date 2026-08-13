"""Base classes for server/gateway implementations"""

from _typeshed import OptExcInfo
from _typeshed.wsgi import ErrorStream, InputStream, StartResponse, WSGIApplication, WSGIEnvironment
from abc import abstractmethod
from collections.abc import Callable, MutableMapping
from typing import IO

from .headers import Headers
from .util import FileWrapper

__all__ = ["BaseHandler", "SimpleHandler", "BaseCGIHandler", "CGIHandler", "IISCGIHandler", "read_environ"]

def format_date_time(timestamp: float | None) -> str: ...  # undocumented
def read_environ() -> dict[str, str]:
    """Read environment, fixing HTTP variables"""

class BaseHandler:
    """Manage the invocation of a WSGI application"""

    wsgi_version: tuple[int, int]  # undocumented
    wsgi_multithread: bool
    wsgi_multiprocess: bool
    wsgi_run_once: bool

    origin_server: bool
    http_version: str
    server_software: str | None

    os_environ: MutableMapping[str, str]

    wsgi_file_wrapper: type[FileWrapper] | None
    headers_class: type[Headers]  # undocumented

    traceback_limit: int | None
    error_status: str
    error_headers: list[tuple[str, str]]
    error_body: bytes
    def run(self, application: WSGIApplication) -> None:
        """Invoke the application"""

    def setup_environ(self) -> None:
        """Set up the environment for one request"""

    def finish_response(self) -> None:
        """Send any iterable data, then close self and the iterable

        Subclasses intended for use in asynchronous servers will
        want to redefine this method, such that it sets up callbacks
        in the event loop to iterate over the data, and to call
        'self.close()' once the response is finished.
        """

    def get_scheme(self) -> str:
        """Return the URL scheme being used"""

    def set_content_length(self) -> None:
        """Compute Content-Length or switch to chunked encoding if possible"""

    def cleanup_headers(self) -> None:
        """Make any necessary header changes or defaults

        Subclasses can extend this to add other defaults.
        """

    def start_response(
        self, status: str, headers: list[tuple[str, str]], exc_info: OptExcInfo | None = None
    ) -> Callable[[bytes], None]:
        """'start_response()' callable as specified by PEP 3333"""

    def send_preamble(self) -> None:
        """Transmit version/status/date/server, via self._write()"""

    def write(self, data: bytes) -> None:
        """'write()' callable as specified by PEP 3333"""

    def sendfile(self) -> bool:
        """Platform-specific file transmission

        Override this method in subclasses to support platform-specific
        file transmission.  It is only called if the application's
        return iterable ('self.result') is an instance of
        'self.wsgi_file_wrapper'.

        This method should return a true value if it was able to actually
        transmit the wrapped file-like object using a platform-specific
        approach.  It should return a false value if normal iteration
        should be used instead.  An exception can be raised to indicate
        that transmission was attempted, but failed.

        NOTE: this method should call 'self.send_headers()' if
        'self.headers_sent' is false and it is going to attempt direct
        transmission of the file.
        """

    def finish_content(self) -> None:
        """Ensure headers and content have both been sent"""

    def close(self) -> None:
        """Close the iterable (if needed) and reset all instance vars

        Subclasses may want to also drop the client connection.
        """

    def send_headers(self) -> None:
        """Transmit headers to the client, via self._write()"""

    def result_is_file(self) -> bool:
        """True if 'self.result' is an instance of 'self.wsgi_file_wrapper'"""

    def client_is_modern(self) -> bool:
        """True if client can accept status and headers"""

    def log_exception(self, exc_info: OptExcInfo) -> None:
        """Log the 'exc_info' tuple in the server log

        Subclasses may override to retarget the output or change its format.
        """

    def handle_error(self) -> None:
        """Log current error, and send error output to client if possible"""

    def error_output(self, environ: WSGIEnvironment, start_response: StartResponse) -> list[bytes]:
        """WSGI mini-app to create error output

        By default, this just uses the 'error_status', 'error_headers',
        and 'error_body' attributes to generate an output page.  It can
        be overridden in a subclass to dynamically generate diagnostics,
        choose an appropriate message for the user's preferred language, etc.

        Note, however, that it's not recommended from a security perspective to
        spit out diagnostics to any old user; ideally, you should have to do
        something special to enable diagnostic output, which is why we don't
        include any here!
        """

    @abstractmethod
    def _write(self, data: bytes) -> None:
        """Override in subclass to buffer data for send to client

        It's okay if this method actually transmits the data; BaseHandler
        just separates write and flush operations for greater efficiency
        when the underlying system actually has such a distinction.
        """

    @abstractmethod
    def _flush(self) -> None:
        """Override in subclass to force sending of recent '_write()' calls

        It's okay if this method is a no-op (i.e., if '_write()' actually
        sends the data.
        """

    @abstractmethod
    def get_stdin(self) -> InputStream:
        """Override in subclass to return suitable 'wsgi.input'"""

    @abstractmethod
    def get_stderr(self) -> ErrorStream:
        """Override in subclass to return suitable 'wsgi.errors'"""

    @abstractmethod
    def add_cgi_vars(self) -> None:
        """Override in subclass to insert CGI variables in 'self.environ'"""

class SimpleHandler(BaseHandler):
    """Handler that's just initialized with streams, environment, etc.

    This handler subclass is intended for synchronous HTTP/1.0 origin servers,
    and handles sending the entire response output, given the correct inputs.

    Usage::

        handler = SimpleHandler(
            inp,out,err,env, multithread=False, multiprocess=True
        )
        handler.run(app)
    """

    stdin: InputStream
    stdout: IO[bytes]
    stderr: ErrorStream
    base_env: MutableMapping[str, str]
    def __init__(
        self,
        stdin: InputStream,
        stdout: IO[bytes],
        stderr: ErrorStream,
        environ: MutableMapping[str, str],
        multithread: bool = True,
        multiprocess: bool = False,
    ) -> None: ...
    def get_stdin(self) -> InputStream: ...
    def get_stderr(self) -> ErrorStream: ...
    def add_cgi_vars(self) -> None: ...
    def _write(self, data: bytes) -> None: ...
    def _flush(self) -> None: ...

class BaseCGIHandler(SimpleHandler):
    """CGI-like systems using input/output/error streams and environ mapping

    Usage::

        handler = BaseCGIHandler(inp,out,err,env)
        handler.run(app)

    This handler class is useful for gateway protocols like ReadyExec and
    FastCGI, that have usable input/output/error streams and an environment
    mapping.  It's also the base class for CGIHandler, which just uses
    sys.stdin, os.environ, and so on.

    The constructor also takes keyword arguments 'multithread' and
    'multiprocess' (defaulting to 'True' and 'False' respectively) to control
    the configuration sent to the application.  It sets 'origin_server' to
    False (to enable CGI-like output), and assumes that 'wsgi.run_once' is
    False.
    """

class CGIHandler(BaseCGIHandler):
    """CGI-based invocation via sys.stdin/stdout/stderr and os.environ

    Usage::

        CGIHandler().run(app)

    The difference between this class and BaseCGIHandler is that it always
    uses 'wsgi.run_once' of 'True', 'wsgi.multithread' of 'False', and
    'wsgi.multiprocess' of 'True'.  It does not take any initialization
    parameters, but always uses 'sys.stdin', 'os.environ', and friends.

    If you need to override any of these parameters, use BaseCGIHandler
    instead.
    """

    def __init__(self) -> None: ...

class IISCGIHandler(BaseCGIHandler):
    """CGI-based invocation with workaround for IIS path bug

    This handler should be used in preference to CGIHandler when deploying on
    Microsoft IIS without having set the config allowPathInfo option (IIS>=7)
    or metabase allowPathInfoForScriptMappings (IIS<7).
    """

    def __init__(self) -> None: ...
