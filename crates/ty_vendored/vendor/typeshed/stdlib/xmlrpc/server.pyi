"""XML-RPC Servers.

This module can be used to create simple XML-RPC servers
by creating a server and either installing functions, a
class instance, or by extending the SimpleXMLRPCServer
class.

It can also be used to handle XML-RPC requests in a CGI
environment using CGIXMLRPCRequestHandler.

The Doc* classes can be used to create XML-RPC servers that
serve pydoc-style documentation in response to HTTP
GET requests. This documentation is dynamically generated
based on the functions and methods registered with the
server.

A list of possible usage patterns follows:

1. Install functions:

server = SimpleXMLRPCServer(("localhost", 8000))
server.register_function(pow)
server.register_function(lambda x,y: x+y, 'add')
server.serve_forever()

2. Install an instance:

class MyFuncs:
    def __init__(self):
        # make all of the sys functions available through sys.func_name
        import sys
        self.sys = sys
    def _listMethods(self):
        # implement this method so that system.listMethods
        # knows to advertise the sys methods
        return list_public_methods(self) + \\
                ['sys.' + method for method in list_public_methods(self.sys)]
    def pow(self, x, y): return pow(x, y)
    def add(self, x, y) : return x + y

server = SimpleXMLRPCServer(("localhost", 8000))
server.register_introspection_functions()
server.register_instance(MyFuncs())
server.serve_forever()

3. Install an instance with custom dispatch method:

class Math:
    def _listMethods(self):
        # this method must be present for system.listMethods
        # to work
        return ['add', 'pow']
    def _methodHelp(self, method):
        # this method must be present for system.methodHelp
        # to work
        if method == 'add':
            return "add(2,3) => 5"
        elif method == 'pow':
            return "pow(x, y[, z]) => number"
        else:
            # By convention, return empty
            # string if no help is available
            return ""
    def _dispatch(self, method, params):
        if method == 'pow':
            return pow(*params)
        elif method == 'add':
            return params[0] + params[1]
        else:
            raise ValueError('bad method')

server = SimpleXMLRPCServer(("localhost", 8000))
server.register_introspection_functions()
server.register_instance(Math())
server.serve_forever()

4. Subclass SimpleXMLRPCServer:

class MathServer(SimpleXMLRPCServer):
    def _dispatch(self, method, params):
        try:
            # We are forcing the 'export_' prefix on methods that are
            # callable through XML-RPC to prevent potential security
            # problems
            func = getattr(self, 'export_' + method)
        except AttributeError:
            raise Exception('method "%s" is not supported' % method)
        else:
            return func(*params)

    def export_add(self, x, y):
        return x + y

server = MathServer(("localhost", 8000))
server.serve_forever()

5. CGI script:

server = CGIXMLRPCRequestHandler()
server.register_function(pow)
server.handle_request()
"""

import http.server
import pydoc
import socketserver
from _typeshed import ReadableBuffer
from collections.abc import Callable, Iterable, Mapping
from re import Pattern
from typing import Any, ClassVar, Protocol, type_check_only
from typing_extensions import TypeAlias
from xmlrpc.client import Fault, _Marshallable

# The dispatch accepts anywhere from 0 to N arguments, no easy way to allow this in mypy
@type_check_only
class _DispatchArity0(Protocol):
    def __call__(self) -> _Marshallable: ...

@type_check_only
class _DispatchArity1(Protocol):
    def __call__(self, arg1: _Marshallable, /) -> _Marshallable: ...

@type_check_only
class _DispatchArity2(Protocol):
    def __call__(self, arg1: _Marshallable, arg2: _Marshallable, /) -> _Marshallable: ...

@type_check_only
class _DispatchArity3(Protocol):
    def __call__(self, arg1: _Marshallable, arg2: _Marshallable, arg3: _Marshallable, /) -> _Marshallable: ...

@type_check_only
class _DispatchArity4(Protocol):
    def __call__(
        self, arg1: _Marshallable, arg2: _Marshallable, arg3: _Marshallable, arg4: _Marshallable, /
    ) -> _Marshallable: ...

@type_check_only
class _DispatchArityN(Protocol):
    def __call__(self, *args: _Marshallable) -> _Marshallable: ...

_DispatchProtocol: TypeAlias = (
    _DispatchArity0 | _DispatchArity1 | _DispatchArity2 | _DispatchArity3 | _DispatchArity4 | _DispatchArityN
)

def resolve_dotted_attribute(obj: Any, attr: str, allow_dotted_names: bool = True) -> Any:  # undocumented
    """resolve_dotted_attribute(a, 'b.c.d') => a.b.c.d

    Resolves a dotted attribute name to an object.  Raises
    an AttributeError if any attribute in the chain starts with a '_'.

    If the optional allow_dotted_names argument is false, dots are not
    supported and this function operates similar to getattr(obj, attr).
    """

def list_public_methods(obj: Any) -> list[str]:  # undocumented
    """Returns a list of attribute strings, found in the specified
    object, which represent callable attributes
    """

class SimpleXMLRPCDispatcher:  # undocumented
    """Mix-in class that dispatches XML-RPC requests.

    This class is used to register XML-RPC method handlers
    and then to dispatch them. This class doesn't need to be
    instanced directly when used by SimpleXMLRPCServer but it
    can be instanced when used by the MultiPathXMLRPCServer
    """

    funcs: dict[str, _DispatchProtocol]
    instance: Any | None
    allow_none: bool
    encoding: str
    use_builtin_types: bool
    def __init__(self, allow_none: bool = False, encoding: str | None = None, use_builtin_types: bool = False) -> None: ...
    def register_instance(self, instance: Any, allow_dotted_names: bool = False) -> None:
        """Registers an instance to respond to XML-RPC requests.

        Only one instance can be installed at a time.

        If the registered instance has a _dispatch method then that
        method will be called with the name of the XML-RPC method and
        its parameters as a tuple
        e.g. instance._dispatch('add',(2,3))

        If the registered instance does not have a _dispatch method
        then the instance will be searched to find a matching method
        and, if found, will be called. Methods beginning with an '_'
        are considered private and will not be called by
        SimpleXMLRPCServer.

        If a registered function matches an XML-RPC request, then it
        will be called instead of the registered instance.

        If the optional allow_dotted_names argument is true and the
        instance does not have a _dispatch method, method names
        containing dots are supported and resolved, as long as none of
        the name segments start with an '_'.

            *** SECURITY WARNING: ***

            Enabling the allow_dotted_names options allows intruders
            to access your module's global variables and may allow
            intruders to execute arbitrary code on your machine.  Only
            use this option on a secure, closed network.

        """

    def register_function(self, function: _DispatchProtocol | None = None, name: str | None = None) -> Callable[..., Any]:
        """Registers a function to respond to XML-RPC requests.

        The optional name argument can be used to set a Unicode name
        for the function.
        """

    def register_introspection_functions(self) -> None:
        """Registers the XML-RPC introspection methods in the system
        namespace.

        see http://xmlrpc.usefulinc.com/doc/reserved.html
        """

    def register_multicall_functions(self) -> None:
        """Registers the XML-RPC multicall method in the system
        namespace.

        see http://www.xmlrpc.com/discuss/msgReader$1208
        """

    def _marshaled_dispatch(
        self,
        data: str | ReadableBuffer,
        dispatch_method: Callable[[str, tuple[_Marshallable, ...]], Fault | tuple[_Marshallable, ...]] | None = None,
        path: Any | None = None,
    ) -> str:  # undocumented
        """Dispatches an XML-RPC method from marshalled (XML) data.

        XML-RPC methods are dispatched from the marshalled (XML) data
        using the _dispatch method and the result is returned as
        marshalled data. For backwards compatibility, a dispatch
        function can be provided as an argument (see comment in
        SimpleXMLRPCRequestHandler.do_POST) but overriding the
        existing method through subclassing is the preferred means
        of changing method dispatch behavior.
        """

    def system_listMethods(self) -> list[str]:  # undocumented
        """system.listMethods() => ['add', 'subtract', 'multiple']

        Returns a list of the methods supported by the server.
        """

    def system_methodSignature(self, method_name: str) -> str:  # undocumented
        """system.methodSignature('add') => [double, int, int]

        Returns a list describing the signature of the method. In the
        above example, the add method takes two integers as arguments
        and returns a double result.

        This server does NOT support system.methodSignature.
        """

    def system_methodHelp(self, method_name: str) -> str:  # undocumented
        """system.methodHelp('add') => "Adds two integers together"

        Returns a string containing documentation for the specified method.
        """

    def system_multicall(self, call_list: list[dict[str, _Marshallable]]) -> list[_Marshallable]:  # undocumented
        """system.multicall([{'methodName': 'add', 'params': [2, 2]}, ...]) => [[4], ...]

        Allows the caller to package multiple XML-RPC calls into a single
        request.

        See http://www.xmlrpc.com/discuss/msgReader$1208
        """

    def _dispatch(self, method: str, params: Iterable[_Marshallable]) -> _Marshallable:  # undocumented
        """Dispatches the XML-RPC method.

        XML-RPC calls are forwarded to a registered function that
        matches the called XML-RPC method name. If no such function
        exists then the call is forwarded to the registered instance,
        if available.

        If the registered instance has a _dispatch method then that
        method will be called with the name of the XML-RPC method and
        its parameters as a tuple
        e.g. instance._dispatch('add',(2,3))

        If the registered instance does not have a _dispatch method
        then the instance will be searched to find a matching method
        and, if found, will be called.

        Methods beginning with an '_' are considered private and will
        not be called.
        """

class SimpleXMLRPCRequestHandler(http.server.BaseHTTPRequestHandler):
    """Simple XML-RPC request handler class.

    Handles all HTTP POST requests and attempts to decode them as
    XML-RPC requests.
    """

    rpc_paths: ClassVar[tuple[str, ...]]
    encode_threshold: int  # undocumented
    aepattern: Pattern[str]  # undocumented
    def accept_encodings(self) -> dict[str, float]: ...
    def is_rpc_path_valid(self) -> bool: ...
    def do_POST(self) -> None:
        """Handles the HTTP POST request.

        Attempts to interpret all HTTP POST requests as XML-RPC calls,
        which are forwarded to the server's _dispatch method for handling.
        """

    def decode_request_content(self, data: bytes) -> bytes | None: ...
    def report_404(self) -> None: ...

class SimpleXMLRPCServer(socketserver.TCPServer, SimpleXMLRPCDispatcher):
    """Simple XML-RPC server.

    Simple XML-RPC server that allows functions and a single instance
    to be installed to handle requests. The default implementation
    attempts to dispatch XML-RPC calls to the functions or instance
    installed in the server. Override the _dispatch method inherited
    from SimpleXMLRPCDispatcher to change this behavior.
    """

    _send_traceback_handler: bool
    def __init__(
        self,
        addr: tuple[str, int],
        requestHandler: type[SimpleXMLRPCRequestHandler] = ...,
        logRequests: bool = True,
        allow_none: bool = False,
        encoding: str | None = None,
        bind_and_activate: bool = True,
        use_builtin_types: bool = False,
    ) -> None: ...

class MultiPathXMLRPCServer(SimpleXMLRPCServer):  # undocumented
    """Multipath XML-RPC Server
    This specialization of SimpleXMLRPCServer allows the user to create
    multiple Dispatcher instances and assign them to different
    HTTP request paths.  This makes it possible to run two or more
    'virtual XML-RPC servers' at the same port.
    Make sure that the requestHandler accepts the paths in question.
    """

    dispatchers: dict[str, SimpleXMLRPCDispatcher]
    def __init__(
        self,
        addr: tuple[str, int],
        requestHandler: type[SimpleXMLRPCRequestHandler] = ...,
        logRequests: bool = True,
        allow_none: bool = False,
        encoding: str | None = None,
        bind_and_activate: bool = True,
        use_builtin_types: bool = False,
    ) -> None: ...
    def add_dispatcher(self, path: str, dispatcher: SimpleXMLRPCDispatcher) -> SimpleXMLRPCDispatcher: ...
    def get_dispatcher(self, path: str) -> SimpleXMLRPCDispatcher: ...

class CGIXMLRPCRequestHandler(SimpleXMLRPCDispatcher):
    """Simple handler for XML-RPC data passed through CGI."""

    def __init__(self, allow_none: bool = False, encoding: str | None = None, use_builtin_types: bool = False) -> None: ...
    def handle_xmlrpc(self, request_text: str) -> None:
        """Handle a single XML-RPC request"""

    def handle_get(self) -> None:
        """Handle a single HTTP GET request.

        Default implementation indicates an error because
        XML-RPC uses the POST method.
        """

    def handle_request(self, request_text: str | None = None) -> None:
        """Handle a single XML-RPC request passed through a CGI post method.

        If no XML data is given then it is read from stdin. The resulting
        XML-RPC response is printed to stdout along with the correct HTTP
        headers.
        """

class ServerHTMLDoc(pydoc.HTMLDoc):  # undocumented
    """Class used to generate pydoc HTML document for a server"""

    def docroutine(  # type: ignore[override]
        self,
        object: object,
        name: str,
        mod: str | None = None,
        funcs: Mapping[str, str] = {},
        classes: Mapping[str, str] = {},
        methods: Mapping[str, str] = {},
        cl: type | None = None,
    ) -> str:
        """Produce HTML documentation for a function or method object."""

    def docserver(self, server_name: str, package_documentation: str, methods: dict[str, str]) -> str:
        """Produce HTML documentation for an XML-RPC server."""

class XMLRPCDocGenerator:  # undocumented
    """Generates documentation for an XML-RPC server.

    This class is designed as mix-in and should not
    be constructed directly.
    """

    server_name: str
    server_documentation: str
    server_title: str
    def set_server_title(self, server_title: str) -> None:
        """Set the HTML title of the generated server documentation"""

    def set_server_name(self, server_name: str) -> None:
        """Set the name of the generated HTML server documentation"""

    def set_server_documentation(self, server_documentation: str) -> None:
        """Set the documentation string for the entire server."""

    def generate_html_documentation(self) -> str:
        """generate_html_documentation() => html documentation for the server

        Generates HTML documentation for the server using introspection for
        installed functions and instances that do not implement the
        _dispatch method. Alternatively, instances can choose to implement
        the _get_method_argstring(method_name) method to provide the
        argument string used in the documentation and the
        _methodHelp(method_name) method to provide the help text used
        in the documentation.
        """

class DocXMLRPCRequestHandler(SimpleXMLRPCRequestHandler):
    """XML-RPC and documentation request handler class.

    Handles all HTTP POST requests and attempts to decode them as
    XML-RPC requests.

    Handles all HTTP GET requests and interprets them as requests
    for documentation.
    """

    def do_GET(self) -> None:
        """Handles the HTTP GET request.

        Interpret all HTTP GET requests as requests for server
        documentation.
        """

class DocXMLRPCServer(SimpleXMLRPCServer, XMLRPCDocGenerator):
    """XML-RPC and HTML documentation server.

    Adds the ability to serve server documentation to the capabilities
    of SimpleXMLRPCServer.
    """

    def __init__(
        self,
        addr: tuple[str, int],
        requestHandler: type[SimpleXMLRPCRequestHandler] = ...,
        logRequests: bool = True,
        allow_none: bool = False,
        encoding: str | None = None,
        bind_and_activate: bool = True,
        use_builtin_types: bool = False,
    ) -> None: ...

class DocCGIXMLRPCRequestHandler(CGIXMLRPCRequestHandler, XMLRPCDocGenerator):
    """Handler for XML-RPC data and documentation requests passed through
    CGI
    """

    def __init__(self) -> None: ...
