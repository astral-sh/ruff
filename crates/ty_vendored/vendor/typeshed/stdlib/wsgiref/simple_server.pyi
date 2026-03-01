"""BaseHTTPServer that implements the Python WSGI protocol (PEP 3333)

This is both an example of how WSGI can be implemented, and a basis for running
simple web applications on a local machine, such as might be done when testing
or debugging an application.  It has not been reviewed for security issues,
however, and we strongly recommend that you use a "real" web server for
production use.

For example usage, see the 'if __name__=="__main__"' block at the end of the
module.  See also the BaseHTTPServer module docs for other API information.
"""

from _typeshed.wsgi import ErrorStream, StartResponse, WSGIApplication, WSGIEnvironment
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Final, TypeVar, overload

from .handlers import SimpleHandler

__all__ = ["WSGIServer", "WSGIRequestHandler", "demo_app", "make_server"]

server_version: Final[str]  # undocumented
sys_version: Final[str]  # undocumented
software_version: Final[str]  # undocumented

class ServerHandler(SimpleHandler):  # undocumented
    server_software: str

class WSGIServer(HTTPServer):
    """BaseHTTPServer that implements the Python WSGI protocol"""

    application: WSGIApplication | None
    base_environ: WSGIEnvironment  # only available after call to setup_environ()
    def setup_environ(self) -> None: ...
    def get_app(self) -> WSGIApplication | None: ...
    def set_app(self, application: WSGIApplication | None) -> None: ...

class WSGIRequestHandler(BaseHTTPRequestHandler):
    server_version: str
    def get_environ(self) -> WSGIEnvironment: ...
    def get_stderr(self) -> ErrorStream: ...

def demo_app(environ: WSGIEnvironment, start_response: StartResponse) -> list[bytes]: ...

_S = TypeVar("_S", bound=WSGIServer)

@overload
def make_server(host: str, port: int, app: WSGIApplication, *, handler_class: type[WSGIRequestHandler] = ...) -> WSGIServer:
    """Create a new WSGI server listening on `host` and `port` for `app`"""

@overload
def make_server(
    host: str, port: int, app: WSGIApplication, server_class: type[_S], handler_class: type[WSGIRequestHandler] = ...
) -> _S: ...
