"""An extensible library for opening URLs using a variety of protocols

The simplest way to use this module is to call the urlopen function,
which accepts a string containing a URL or a Request object (described
below).  It opens the URL and returns the results as file-like
object; the returned object has some extra methods described below.

The OpenerDirector manages a collection of Handler objects that do
all the actual work.  Each Handler implements a particular protocol or
option.  The OpenerDirector is a composite object that invokes the
Handlers needed to open the requested URL.  For example, the
HTTPHandler performs HTTP GET and POST requests and deals with
non-error returns.  The HTTPRedirectHandler automatically deals with
HTTP 301, 302, 303, 307, and 308 redirect errors, and the
HTTPDigestAuthHandler deals with digest authentication.

urlopen(url, data=None) -- Basic usage is the same as original
urllib.  pass the url and optionally data to post to an HTTP URL, and
get a file-like object back.  One difference is that you can also pass
a Request instance instead of URL.  Raises a URLError (subclass of
OSError); for HTTP errors, raises an HTTPError, which can also be
treated as a valid response.

build_opener -- Function that creates a new OpenerDirector instance.
Will install the default handlers.  Accepts one or more Handlers as
arguments, either instances or Handler classes that it will
instantiate.  If one of the argument is a subclass of the default
handler, the argument will be installed instead of the default.

install_opener -- Installs a new opener as the default opener.

objects of interest:

OpenerDirector -- Sets up the User Agent as the Python-urllib client and manages
the Handler classes, while dealing with requests and responses.

Request -- An object that encapsulates the state of a request.  The
state can be as simple as the URL.  It can also include extra HTTP
headers, e.g. a User-Agent.

BaseHandler --

internals:
BaseHandler and parent
_call_chain conventions

Example usage:

import urllib.request

# set up authentication info
authinfo = urllib.request.HTTPBasicAuthHandler()
authinfo.add_password(realm='PDQ Application',
                      uri='https://mahler:8092/site-updates.py',
                      user='klem',
                      passwd='geheim$parole')

proxy_support = urllib.request.ProxyHandler({"http" : "http://ahad-haam:3128"})

# build a new opener that adds authentication and caching FTP handlers
opener = urllib.request.build_opener(proxy_support, authinfo,
                                     urllib.request.CacheFTPHandler)

# install it
urllib.request.install_opener(opener)

f = urllib.request.urlopen('https://www.python.org/')
"""

import ssl
import sys
from _typeshed import ReadableBuffer, StrOrBytesPath, SupportsRead
from collections.abc import Callable, Iterable, Mapping, MutableMapping, Sequence
from email.message import Message
from http.client import HTTPConnection, HTTPMessage, HTTPResponse
from http.cookiejar import CookieJar
from re import Pattern
from typing import IO, Any, ClassVar, NoReturn, Protocol, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, deprecated
from urllib.error import HTTPError as HTTPError
from urllib.response import addclosehook, addinfourl

__all__ = [
    "Request",
    "OpenerDirector",
    "BaseHandler",
    "HTTPDefaultErrorHandler",
    "HTTPRedirectHandler",
    "HTTPCookieProcessor",
    "ProxyHandler",
    "HTTPPasswordMgr",
    "HTTPPasswordMgrWithDefaultRealm",
    "HTTPPasswordMgrWithPriorAuth",
    "AbstractBasicAuthHandler",
    "HTTPBasicAuthHandler",
    "ProxyBasicAuthHandler",
    "AbstractDigestAuthHandler",
    "HTTPDigestAuthHandler",
    "ProxyDigestAuthHandler",
    "HTTPHandler",
    "FileHandler",
    "FTPHandler",
    "CacheFTPHandler",
    "DataHandler",
    "UnknownHandler",
    "HTTPErrorProcessor",
    "urlopen",
    "install_opener",
    "build_opener",
    "pathname2url",
    "url2pathname",
    "getproxies",
    "urlretrieve",
    "urlcleanup",
    "HTTPSHandler",
]
if sys.version_info < (3, 14):
    __all__ += ["URLopener", "FancyURLopener"]

_T = TypeVar("_T")

# The actual type is `addinfourl | HTTPResponse`, but users would need to use `typing.cast` or `isinstance` to narrow the type,
# so we use `Any` instead.
# See
# - https://github.com/python/typeshed/pull/15042
# - https://github.com/python/typing/issues/566
_UrlopenRet: TypeAlias = Any

_DataType: TypeAlias = ReadableBuffer | SupportsRead[bytes] | Iterable[bytes] | None

if sys.version_info >= (3, 13):
    def urlopen(
        url: str | Request, data: _DataType | None = None, timeout: float | None = ..., *, context: ssl.SSLContext | None = None
    ) -> _UrlopenRet:
        """Open the URL url, which can be either a string or a Request object.

        *data* must be an object specifying additional data to be sent to
        the server, or None if no such data is needed.  See Request for
        details.

        urllib.request module uses HTTP/1.1 and includes a "Connection:close"
        header in its HTTP requests.

        The optional *timeout* parameter specifies a timeout in seconds for
        blocking operations like the connection attempt (if not specified, the
        global default timeout setting will be used). This only works for HTTP,
        HTTPS and FTP connections.

        If *context* is specified, it must be a ssl.SSLContext instance describing
        the various SSL options. See HTTPSConnection for more details.


        This function always returns an object which can work as a
        context manager and has the properties url, headers, and status.
        See urllib.response.addinfourl for more detail on these properties.

        For HTTP and HTTPS URLs, this function returns a http.client.HTTPResponse
        object slightly modified. In addition to the three new methods above, the
        msg attribute contains the same information as the reason attribute ---
        the reason phrase returned by the server --- instead of the response
        headers as it is specified in the documentation for HTTPResponse.

        For FTP, file, and data URLs, this function returns a
        urllib.response.addinfourl object.

        Note that None may be returned if no handler handles the request (though
        the default installed global OpenerDirector uses UnknownHandler to ensure
        this never happens).

        In addition, if proxy settings are detected (for example, when a *_proxy
        environment variable like http_proxy is set), ProxyHandler is default
        installed and makes sure the requests are handled through the proxy.

        """

else:
    def urlopen(
        url: str | Request,
        data: _DataType | None = None,
        timeout: float | None = ...,
        *,
        cafile: str | None = None,
        capath: str | None = None,
        cadefault: bool = False,
        context: ssl.SSLContext | None = None,
    ) -> _UrlopenRet:
        """Open the URL url, which can be either a string or a Request object.

        *data* must be an object specifying additional data to be sent to
        the server, or None if no such data is needed.  See Request for
        details.

        urllib.request module uses HTTP/1.1 and includes a "Connection:close"
        header in its HTTP requests.

        The optional *timeout* parameter specifies a timeout in seconds for
        blocking operations like the connection attempt (if not specified, the
        global default timeout setting will be used). This only works for HTTP,
        HTTPS and FTP connections.

        If *context* is specified, it must be a ssl.SSLContext instance describing
        the various SSL options. See HTTPSConnection for more details.

        The optional *cafile* and *capath* parameters specify a set of trusted CA
        certificates for HTTPS requests. cafile should point to a single file
        containing a bundle of CA certificates, whereas capath should point to a
        directory of hashed certificate files. More information can be found in
        ssl.SSLContext.load_verify_locations().

        The *cadefault* parameter is ignored.


        This function always returns an object which can work as a
        context manager and has the properties url, headers, and status.
        See urllib.response.addinfourl for more detail on these properties.

        For HTTP and HTTPS URLs, this function returns a http.client.HTTPResponse
        object slightly modified. In addition to the three new methods above, the
        msg attribute contains the same information as the reason attribute ---
        the reason phrase returned by the server --- instead of the response
        headers as it is specified in the documentation for HTTPResponse.

        For FTP, file, and data URLs and requests explicitly handled by legacy
        URLopener and FancyURLopener classes, this function returns a
        urllib.response.addinfourl object.

        Note that None may be returned if no handler handles the request (though
        the default installed global OpenerDirector uses UnknownHandler to ensure
        this never happens).

        In addition, if proxy settings are detected (for example, when a *_proxy
        environment variable like http_proxy is set), ProxyHandler is default
        installed and makes sure the requests are handled through the proxy.

        """

def install_opener(opener: OpenerDirector) -> None: ...
def build_opener(*handlers: BaseHandler | Callable[[], BaseHandler]) -> OpenerDirector:
    """Create an opener object from a list of handlers.

    The opener will use several default handlers, including support
    for HTTP, FTP and when applicable HTTPS.

    If any of the handlers passed as arguments are subclasses of the
    default handlers, the default handlers will not be used.
    """

if sys.version_info >= (3, 14):
    def url2pathname(url: str, *, require_scheme: bool = False, resolve_host: bool = False) -> str:
        """Convert the given file URL to a local file system path.

        The 'file:' scheme prefix must be omitted unless *require_scheme*
        is set to true.

        The URL authority may be resolved with gethostbyname() if
        *resolve_host* is set to true.
        """

    def pathname2url(pathname: str, *, add_scheme: bool = False) -> str:
        """Convert the given local file system path to a file URL.

        The 'file:' scheme prefix is omitted unless *add_scheme*
        is set to true.
        """

else:
    if sys.platform == "win32":
        from nturl2path import pathname2url as pathname2url, url2pathname as url2pathname
    else:
        def url2pathname(pathname: str) -> str:
            """OS-specific conversion from a relative URL of the 'file' scheme
            to a file system path; not recommended for general use.
            """

        def pathname2url(pathname: str) -> str:
            """OS-specific conversion from a file system path to a relative URL
            of the 'file' scheme; not recommended for general use.
            """

def getproxies() -> dict[str, str]:
    """Return a dictionary of scheme -> proxy server URL mappings.

    Scan the environment for variables named <scheme>_proxy;
    this seems to be the standard convention.
    """

def getproxies_environment() -> dict[str, str]:
    """Return a dictionary of scheme -> proxy server URL mappings.

    Scan the environment for variables named <scheme>_proxy;
    this seems to be the standard convention.
    """

def parse_http_list(s: str) -> list[str]:
    """Parse lists as described by RFC 2068 Section 2.

    In particular, parse comma-separated lists where the elements of
    the list may include quoted-strings.  A quoted-string could
    contain a comma.  A non-quoted string could have quotes in the
    middle.  Neither commas nor quotes count if they are escaped.
    Only double-quotes count, not single-quotes.
    """

def parse_keqv_list(l: list[str]) -> dict[str, str]:
    """Parse list of key=value strings where keys are not duplicated."""

if sys.platform == "win32" or sys.platform == "darwin":
    def proxy_bypass(host: str) -> Any:  # undocumented
        """Return True, if host should be bypassed.

        Checks proxy settings gathered from the environment, if specified,
        or the registry.

        """

else:
    def proxy_bypass(host: str, proxies: Mapping[str, str] | None = None) -> Any:  # undocumented
        """Test if proxies should not be used for a particular host.

        Checks the proxy dict for the value of no_proxy, which should
        be a list of comma separated DNS suffixes, or '*' for all hosts.

        """

class Request:
    @property
    def full_url(self) -> str: ...
    @full_url.setter
    def full_url(self, value: str) -> None: ...
    @full_url.deleter
    def full_url(self) -> None: ...
    type: str
    host: str
    origin_req_host: str
    selector: str
    data: _DataType
    headers: MutableMapping[str, str]
    unredirected_hdrs: dict[str, str]
    unverifiable: bool
    method: str | None
    timeout: float | None  # Undocumented, only set after __init__() by OpenerDirector.open()
    def __init__(
        self,
        url: str,
        data: _DataType = None,
        headers: MutableMapping[str, str] = {},
        origin_req_host: str | None = None,
        unverifiable: bool = False,
        method: str | None = None,
    ) -> None: ...
    def get_method(self) -> str:
        """Return a string indicating the HTTP request method."""

    def add_header(self, key: str, val: str) -> None: ...
    def add_unredirected_header(self, key: str, val: str) -> None: ...
    def has_header(self, header_name: str) -> bool: ...
    def remove_header(self, header_name: str) -> None: ...
    def get_full_url(self) -> str: ...
    def set_proxy(self, host: str, type: str) -> None: ...
    @overload
    def get_header(self, header_name: str) -> str | None: ...
    @overload
    def get_header(self, header_name: str, default: _T) -> str | _T: ...
    def header_items(self) -> list[tuple[str, str]]: ...
    def has_proxy(self) -> bool: ...

class OpenerDirector:
    addheaders: list[tuple[str, str]]
    def add_handler(self, handler: BaseHandler) -> None: ...
    def open(self, fullurl: str | Request, data: _DataType = None, timeout: float | None = ...) -> _UrlopenRet: ...
    def error(self, proto: str, *args: Any) -> _UrlopenRet: ...
    def close(self) -> None: ...

class BaseHandler:
    handler_order: ClassVar[int]
    parent: OpenerDirector
    def add_parent(self, parent: OpenerDirector) -> None: ...
    def close(self) -> None: ...
    def __lt__(self, other: object) -> bool: ...

class HTTPDefaultErrorHandler(BaseHandler):
    def http_error_default(
        self, req: Request, fp: IO[bytes], code: int, msg: str, hdrs: HTTPMessage
    ) -> HTTPError: ...  # undocumented

class HTTPRedirectHandler(BaseHandler):
    max_redirections: ClassVar[int]  # undocumented
    max_repeats: ClassVar[int]  # undocumented
    inf_msg: ClassVar[str]  # undocumented
    def redirect_request(
        self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage, newurl: str
    ) -> Request | None:
        """Return a Request or None in response to a redirect.

        This is called by the http_error_30x methods when a
        redirection response is received.  If a redirection should
        take place, return a new Request to allow http_error_30x to
        perform the redirect.  Otherwise, raise HTTPError if no-one
        else should try to handle this url.  Return None if you can't
        but another Handler might.
        """

    def http_error_301(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...
    def http_error_302(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...
    def http_error_303(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...
    def http_error_307(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...
    if sys.version_info >= (3, 11):
        def http_error_308(
            self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage
        ) -> _UrlopenRet | None: ...

class HTTPCookieProcessor(BaseHandler):
    cookiejar: CookieJar
    def __init__(self, cookiejar: CookieJar | None = None) -> None: ...
    def http_request(self, request: Request) -> Request: ...  # undocumented
    def http_response(self, request: Request, response: HTTPResponse) -> HTTPResponse: ...  # undocumented
    def https_request(self, request: Request) -> Request: ...  # undocumented
    def https_response(self, request: Request, response: HTTPResponse) -> HTTPResponse: ...  # undocumented

class ProxyHandler(BaseHandler):
    def __init__(self, proxies: dict[str, str] | None = None) -> None: ...
    def proxy_open(self, req: Request, proxy: str, type: str) -> _UrlopenRet | None: ...  # undocumented
    # TODO: add a method for every (common) proxy protocol

class HTTPPasswordMgr:
    def add_password(self, realm: str, uri: str | Sequence[str], user: str, passwd: str) -> None: ...
    def find_user_password(self, realm: str, authuri: str) -> tuple[str | None, str | None]: ...
    def is_suburi(self, base: str, test: str) -> bool:  # undocumented
        """Check if test is below base in a URI tree

        Both args must be URIs in reduced form.
        """

    def reduce_uri(self, uri: str, default_port: bool = True) -> tuple[str, str]:  # undocumented
        """Accept authority or URI and extract only the authority and path."""

class HTTPPasswordMgrWithDefaultRealm(HTTPPasswordMgr):
    def add_password(self, realm: str | None, uri: str | Sequence[str], user: str, passwd: str) -> None: ...
    def find_user_password(self, realm: str | None, authuri: str) -> tuple[str | None, str | None]: ...

class HTTPPasswordMgrWithPriorAuth(HTTPPasswordMgrWithDefaultRealm):
    def add_password(
        self, realm: str | None, uri: str | Sequence[str], user: str, passwd: str, is_authenticated: bool = False
    ) -> None: ...
    def update_authenticated(self, uri: str | Sequence[str], is_authenticated: bool = False) -> None: ...
    def is_authenticated(self, authuri: str) -> bool | None: ...

class AbstractBasicAuthHandler:
    rx: ClassVar[Pattern[str]]  # undocumented
    passwd: HTTPPasswordMgr
    add_password: Callable[[str, str | Sequence[str], str, str], None]
    def __init__(self, password_mgr: HTTPPasswordMgr | None = None) -> None: ...
    def http_error_auth_reqed(self, authreq: str, host: str, req: Request, headers: HTTPMessage) -> None: ...
    def http_request(self, req: Request) -> Request: ...  # undocumented
    def http_response(self, req: Request, response: HTTPResponse) -> HTTPResponse: ...  # undocumented
    def https_request(self, req: Request) -> Request: ...  # undocumented
    def https_response(self, req: Request, response: HTTPResponse) -> HTTPResponse: ...  # undocumented
    def retry_http_basic_auth(self, host: str, req: Request, realm: str) -> _UrlopenRet | None: ...  # undocumented

class HTTPBasicAuthHandler(AbstractBasicAuthHandler, BaseHandler):
    auth_header: ClassVar[str]  # undocumented
    def http_error_401(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...

class ProxyBasicAuthHandler(AbstractBasicAuthHandler, BaseHandler):
    auth_header: ClassVar[str]
    def http_error_407(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...

class AbstractDigestAuthHandler:
    def __init__(self, passwd: HTTPPasswordMgr | None = None) -> None: ...
    def reset_retry_count(self) -> None: ...
    def http_error_auth_reqed(self, auth_header: str, host: str, req: Request, headers: HTTPMessage) -> None: ...
    def retry_http_digest_auth(self, req: Request, auth: str) -> _UrlopenRet | None: ...
    def get_cnonce(self, nonce: str) -> str: ...
    def get_authorization(self, req: Request, chal: Mapping[str, str]) -> str | None: ...
    def get_algorithm_impls(self, algorithm: str) -> tuple[Callable[[str], str], Callable[[str, str], str]]: ...
    def get_entity_digest(self, data: ReadableBuffer | None, chal: Mapping[str, str]) -> str | None: ...

class HTTPDigestAuthHandler(BaseHandler, AbstractDigestAuthHandler):
    """An authentication protocol defined by RFC 2069

    Digest authentication improves on basic authentication because it
    does not transmit passwords in the clear.
    """

    auth_header: ClassVar[str]  # undocumented
    def http_error_401(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...

class ProxyDigestAuthHandler(BaseHandler, AbstractDigestAuthHandler):
    auth_header: ClassVar[str]  # undocumented
    def http_error_407(self, req: Request, fp: IO[bytes], code: int, msg: str, headers: HTTPMessage) -> _UrlopenRet | None: ...

@type_check_only
class _HTTPConnectionProtocol(Protocol):
    def __call__(
        self,
        host: str,
        /,
        *,
        port: int | None = ...,
        timeout: float = ...,
        source_address: tuple[str, int] | None = ...,
        blocksize: int = ...,
    ) -> HTTPConnection: ...

class AbstractHTTPHandler(BaseHandler):  # undocumented
    if sys.version_info >= (3, 12):
        def __init__(self, debuglevel: int | None = None) -> None: ...
    else:
        def __init__(self, debuglevel: int = 0) -> None: ...

    def set_http_debuglevel(self, level: int) -> None: ...
    def do_request_(self, request: Request) -> Request: ...
    def do_open(self, http_class: _HTTPConnectionProtocol, req: Request, **http_conn_args: Any) -> HTTPResponse:
        """Return an HTTPResponse object for the request, using http_class.

        http_class must implement the HTTPConnection API from http.client.
        """

class HTTPHandler(AbstractHTTPHandler):
    def http_open(self, req: Request) -> HTTPResponse: ...
    def http_request(self, request: Request) -> Request: ...  # undocumented

class HTTPSHandler(AbstractHTTPHandler):
    if sys.version_info >= (3, 12):
        def __init__(
            self, debuglevel: int | None = None, context: ssl.SSLContext | None = None, check_hostname: bool | None = None
        ) -> None: ...
    else:
        def __init__(
            self, debuglevel: int = 0, context: ssl.SSLContext | None = None, check_hostname: bool | None = None
        ) -> None: ...

    def https_open(self, req: Request) -> HTTPResponse: ...
    def https_request(self, request: Request) -> Request: ...  # undocumented

class FileHandler(BaseHandler):
    names: ClassVar[tuple[str, ...] | None]  # undocumented
    def file_open(self, req: Request) -> addinfourl: ...
    def get_names(self) -> tuple[str, ...]: ...  # undocumented
    def open_local_file(self, req: Request) -> addinfourl: ...  # undocumented

class DataHandler(BaseHandler):
    def data_open(self, req: Request) -> addinfourl: ...

class ftpwrapper:  # undocumented
    """Class used by open_ftp() for cache of open FTP connections."""

    def __init__(
        self, user: str, passwd: str, host: str, port: int, dirs: str, timeout: float | None = None, persistent: bool = True
    ) -> None: ...
    def close(self) -> None: ...
    def endtransfer(self) -> None: ...
    def file_close(self) -> None: ...
    def init(self) -> None: ...
    def real_close(self) -> None: ...
    def retrfile(self, file: str, type: str) -> tuple[addclosehook, int | None]: ...

class FTPHandler(BaseHandler):
    def ftp_open(self, req: Request) -> addinfourl: ...
    def connect_ftp(
        self, user: str, passwd: str, host: str, port: int, dirs: str, timeout: float
    ) -> ftpwrapper: ...  # undocumented

class CacheFTPHandler(FTPHandler):
    def setTimeout(self, t: float) -> None: ...
    def setMaxConns(self, m: int) -> None: ...
    def check_cache(self) -> None: ...  # undocumented
    def clear_cache(self) -> None: ...  # undocumented

class UnknownHandler(BaseHandler):
    def unknown_open(self, req: Request) -> NoReturn: ...

class HTTPErrorProcessor(BaseHandler):
    """Process HTTP error responses."""

    def http_response(self, request: Request, response: HTTPResponse) -> _UrlopenRet: ...
    def https_response(self, request: Request, response: HTTPResponse) -> _UrlopenRet: ...

def urlretrieve(
    url: str,
    filename: StrOrBytesPath | None = None,
    reporthook: Callable[[int, int, int], object] | None = None,
    data: _DataType = None,
) -> tuple[str, HTTPMessage]:
    """
    Retrieve a URL into a temporary location on disk.

    Requires a URL argument. If a filename is passed, it is used as
    the temporary file location. The reporthook argument should be
    a callable that accepts a block number, a read size, and the
    total file size of the URL target. The data argument should be
    valid URL encoded data.

    If a filename is passed and the URL points to a local resource,
    the result is a copy from local file to new file.

    Returns a tuple containing the path to the newly created
    data file as well as the resulting HTTPMessage object.
    """

def urlcleanup() -> None:
    """Clean up temporary files from urlretrieve calls."""

if sys.version_info < (3, 14):
    @deprecated("Deprecated since Python 3.3; removed in Python 3.14. Use newer `urlopen` functions and methods.")
    class URLopener:
        """Class to open URLs.
        This is a class rather than just a subroutine because we may need
        more than one set of global protocol-specific options.
        Note -- this is a base class for those who don't want the
        automatic handling of errors type 302 (relocated) and 401
        (authorization needed).
        """

        version: ClassVar[str]
        def __init__(self, proxies: dict[str, str] | None = None, **x509: str) -> None: ...
        def open(self, fullurl: str, data: ReadableBuffer | None = None) -> _UrlopenRet:
            """Use URLopener().open(file) instead of open(file, 'r')."""

        def open_unknown(self, fullurl: str, data: ReadableBuffer | None = None) -> _UrlopenRet:
            """Overridable interface to open unknown URL type."""

        def retrieve(
            self,
            url: str,
            filename: str | None = None,
            reporthook: Callable[[int, int, int], object] | None = None,
            data: ReadableBuffer | None = None,
        ) -> tuple[str, Message | None]:
            """retrieve(url) returns (filename, headers) for a local object
            or (tempfilename, headers) for a remote object.
            """

        def addheader(self, *args: tuple[str, str]) -> None:  # undocumented
            """Add a header to be used by the HTTP interface only
            e.g. u.addheader('Accept', 'sound/basic')
            """

        def cleanup(self) -> None: ...  # undocumented
        def close(self) -> None: ...  # undocumented
        def http_error(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: bytes | None = None
        ) -> _UrlopenRet:  # undocumented
            """Handle http errors.

            Derived class can override this, or provide specific handlers
            named http_error_DDD where DDD is the 3-digit error code.
            """

        def http_error_default(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage
        ) -> _UrlopenRet:  # undocumented
            """Default error handler: close the connection and raise OSError."""

        def open_data(self, url: str, data: ReadableBuffer | None = None) -> addinfourl:  # undocumented
            """Use "data" URL."""

        def open_file(self, url: str) -> addinfourl:  # undocumented
            """Use local file or FTP depending on form of URL."""

        def open_ftp(self, url: str) -> addinfourl:  # undocumented
            """Use FTP protocol."""

        def open_http(self, url: str, data: ReadableBuffer | None = None) -> _UrlopenRet:  # undocumented
            """Use HTTP protocol."""

        def open_https(self, url: str, data: ReadableBuffer | None = None) -> _UrlopenRet:  # undocumented
            """Use HTTPS protocol."""

        def open_local_file(self, url: str) -> addinfourl:  # undocumented
            """Use local file."""

        def open_unknown_proxy(self, proxy: str, fullurl: str, data: ReadableBuffer | None = None) -> None:  # undocumented
            """Overridable interface to open unknown URL type."""

        def __del__(self) -> None: ...

    @deprecated("Deprecated since Python 3.3; removed in Python 3.14. Use newer `urlopen` functions and methods.")
    class FancyURLopener(URLopener):
        """Derived class with handlers for errors we can handle (perhaps)."""

        def prompt_user_passwd(self, host: str, realm: str) -> tuple[str, str]:
            """Override this in a GUI environment!"""

        def get_user_passwd(self, host: str, realm: str, clear_cache: int = 0) -> tuple[str, str]: ...  # undocumented
        def http_error_301(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | addinfourl | None:  # undocumented
            """Error 301 -- also relocated (permanently)."""

        def http_error_302(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | addinfourl | None:  # undocumented
            """Error 302 -- relocated (temporarily)."""

        def http_error_303(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | addinfourl | None:  # undocumented
            """Error 303 -- also relocated (essentially identical to 302)."""

        def http_error_307(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | addinfourl | None:  # undocumented
            """Error 307 -- relocated, but turn POST into error."""
        if sys.version_info >= (3, 11):
            def http_error_308(
                self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None = None
            ) -> _UrlopenRet | addinfourl | None:  # undocumented
                """Error 308 -- relocated, but turn POST into error."""

        def http_error_401(
            self,
            url: str,
            fp: IO[bytes],
            errcode: int,
            errmsg: str,
            headers: HTTPMessage,
            data: ReadableBuffer | None = None,
            retry: bool = False,
        ) -> _UrlopenRet | None:  # undocumented
            """Error 401 -- authentication required.
            This function supports Basic authentication only.
            """

        def http_error_407(
            self,
            url: str,
            fp: IO[bytes],
            errcode: int,
            errmsg: str,
            headers: HTTPMessage,
            data: ReadableBuffer | None = None,
            retry: bool = False,
        ) -> _UrlopenRet | None:  # undocumented
            """Error 407 -- proxy authentication required.
            This function supports Basic authentication only.
            """

        def http_error_default(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage
        ) -> addinfourl:  # undocumented
            """Default error handling -- don't raise an exception."""

        def redirect_internal(
            self, url: str, fp: IO[bytes], errcode: int, errmsg: str, headers: HTTPMessage, data: ReadableBuffer | None
        ) -> _UrlopenRet | None: ...  # undocumented
        def retry_http_basic_auth(
            self, url: str, realm: str, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | None: ...  # undocumented
        def retry_https_basic_auth(
            self, url: str, realm: str, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | None: ...  # undocumented
        def retry_proxy_http_basic_auth(
            self, url: str, realm: str, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | None: ...  # undocumented
        def retry_proxy_https_basic_auth(
            self, url: str, realm: str, data: ReadableBuffer | None = None
        ) -> _UrlopenRet | None: ...  # undocumented
