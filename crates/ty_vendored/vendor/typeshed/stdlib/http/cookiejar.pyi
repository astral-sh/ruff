"""HTTP cookie handling for web clients.

This module has (now fairly distant) origins in Gisle Aas' Perl module
HTTP::Cookies, from the libwww-perl library.

Docstrings, comments and debug strings in this code refer to the
attributes of the HTTP cookie system as cookie-attributes, to distinguish
them clearly from Python attributes.

Class diagram (note that BSDDBCookieJar and the MSIE* classes are not
distributed with the Python standard library, but are available from
http://wwwsearch.sf.net/):

                        CookieJar____
                        /     \\      \\
            FileCookieJar      \\      \\
             /    |   \\         \\      \\
 MozillaCookieJar | LWPCookieJar \\      \\
                  |               |      \\
                  |   ---MSIEBase |       \\
                  |  /      |     |        \\
                  | /   MSIEDBCookieJar BSDDBCookieJar
                  |/
               MSIECookieJar

"""

import sys
from _typeshed import StrPath
from collections.abc import Iterator, Sequence
from http.client import HTTPResponse
from re import Pattern
from typing import ClassVar, TypeVar, overload
from urllib.request import Request

__all__ = [
    "Cookie",
    "CookieJar",
    "CookiePolicy",
    "DefaultCookiePolicy",
    "FileCookieJar",
    "LWPCookieJar",
    "LoadError",
    "MozillaCookieJar",
]

_T = TypeVar("_T")

class LoadError(OSError): ...

class CookieJar:
    """Collection of HTTP cookies.

    You may not need to know about this class: try
    urllib.request.build_opener(HTTPCookieProcessor).open(url).
    """

    non_word_re: ClassVar[Pattern[str]]  # undocumented
    quote_re: ClassVar[Pattern[str]]  # undocumented
    strict_domain_re: ClassVar[Pattern[str]]  # undocumented
    domain_re: ClassVar[Pattern[str]]  # undocumented
    dots_re: ClassVar[Pattern[str]]  # undocumented
    magic_re: ClassVar[Pattern[str]]  # undocumented
    def __init__(self, policy: CookiePolicy | None = None) -> None: ...
    def add_cookie_header(self, request: Request) -> None:
        """Add correct Cookie: header to request (urllib.request.Request object).

        The Cookie2 header is also added unless policy.hide_cookie2 is true.

        """

    def extract_cookies(self, response: HTTPResponse, request: Request) -> None:
        """Extract cookies from response, where allowable given the request."""

    def set_policy(self, policy: CookiePolicy) -> None: ...
    def make_cookies(self, response: HTTPResponse, request: Request) -> Sequence[Cookie]:
        """Return sequence of Cookie objects extracted from response object."""

    def set_cookie(self, cookie: Cookie) -> None:
        """Set a cookie, without checking whether or not it should be set."""

    def set_cookie_if_ok(self, cookie: Cookie, request: Request) -> None:
        """Set a cookie if policy says it's OK to do so."""

    def clear(self, domain: str | None = None, path: str | None = None, name: str | None = None) -> None:
        """Clear some cookies.

        Invoking this method without arguments will clear all cookies.  If
        given a single argument, only cookies belonging to that domain will be
        removed.  If given two arguments, cookies belonging to the specified
        path within that domain are removed.  If given three arguments, then
        the cookie with the specified name, path and domain is removed.

        Raises KeyError if no matching cookie exists.

        """

    def clear_session_cookies(self) -> None:
        """Discard all session cookies.

        Note that the .save() method won't save session cookies anyway, unless
        you ask otherwise by passing a true ignore_discard argument.

        """

    def clear_expired_cookies(self) -> None:  # undocumented
        """Discard all expired cookies.

        You probably don't need to call this method: expired cookies are never
        sent back to the server (provided you're using DefaultCookiePolicy),
        this method is called by CookieJar itself every so often, and the
        .save() method won't save expired cookies anyway (unless you ask
        otherwise by passing a true ignore_expires argument).

        """

    def __iter__(self) -> Iterator[Cookie]: ...
    def __len__(self) -> int:
        """Return number of contained cookies."""

class FileCookieJar(CookieJar):
    """CookieJar that can be loaded from and saved to a file."""

    filename: str | None
    delayload: bool
    def __init__(self, filename: StrPath | None = None, delayload: bool = False, policy: CookiePolicy | None = None) -> None:
        """
        Cookies are NOT loaded from the named file until either the .load() or
        .revert() method is called.

        """

    def save(self, filename: str | None = None, ignore_discard: bool = False, ignore_expires: bool = False) -> None:
        """Save cookies to a file."""

    def load(self, filename: str | None = None, ignore_discard: bool = False, ignore_expires: bool = False) -> None:
        """Load cookies from a file."""

    def revert(self, filename: str | None = None, ignore_discard: bool = False, ignore_expires: bool = False) -> None:
        """Clear all cookies and reload cookies from a saved file.

        Raises LoadError (or OSError) if reversion is not successful; the
        object's state will not be altered if this happens.

        """

class MozillaCookieJar(FileCookieJar):
    """

    WARNING: you may want to backup your browser's cookies file if you use
    this class to save cookies.  I *think* it works, but there have been
    bugs in the past!

    This class differs from CookieJar only in the format it uses to save and
    load cookies to and from a file.  This class uses the Mozilla/Netscape
    'cookies.txt' format.  curl and lynx use this file format, too.

    Don't expect cookies saved while the browser is running to be noticed by
    the browser (in fact, Mozilla on unix will overwrite your saved cookies if
    you change them on disk while it's running; on Windows, you probably can't
    save at all while the browser is running).

    Note that the Mozilla/Netscape format will downgrade RFC2965 cookies to
    Netscape cookies on saving.

    In particular, the cookie version and port number information is lost,
    together with information about whether or not Path, Port and Discard were
    specified by the Set-Cookie2 (or Set-Cookie) header, and whether or not the
    domain as set in the HTTP header started with a dot (yes, I'm aware some
    domains in Netscape files start with a dot and some don't -- trust me, you
    really don't want to know any more about this).

    Note that though Mozilla and Netscape use the same format, they use
    slightly different headers.  The class saves cookies using the Netscape
    header by default (Mozilla can cope with that).

    """

    if sys.version_info < (3, 10):
        header: ClassVar[str]  # undocumented

class LWPCookieJar(FileCookieJar):
    """
    The LWPCookieJar saves a sequence of "Set-Cookie3" lines.
    "Set-Cookie3" is the format used by the libwww-perl library, not known
    to be compatible with any browser, but which is easy to read and
    doesn't lose information about RFC 2965 cookies.

    Additional methods

    as_lwp_str(ignore_discard=True, ignore_expired=True)

    """

    def as_lwp_str(self, ignore_discard: bool = True, ignore_expires: bool = True) -> str:  # undocumented
        """Return cookies as a string of "\\n"-separated "Set-Cookie3" headers.

        ignore_discard and ignore_expires: see docstring for FileCookieJar.save

        """

class CookiePolicy:
    """Defines which cookies get accepted from and returned to server.

    May also modify cookies, though this is probably a bad idea.

    The subclass DefaultCookiePolicy defines the standard rules for Netscape
    and RFC 2965 cookies -- override that if you want a customized policy.

    """

    netscape: bool
    rfc2965: bool
    hide_cookie2: bool
    def set_ok(self, cookie: Cookie, request: Request) -> bool:
        """Return true if (and only if) cookie should be accepted from server.

        Currently, pre-expired cookies never get this far -- the CookieJar
        class deletes such cookies itself.

        """

    def return_ok(self, cookie: Cookie, request: Request) -> bool:
        """Return true if (and only if) cookie should be returned to server."""

    def domain_return_ok(self, domain: str, request: Request) -> bool:
        """Return false if cookies should not be returned, given cookie domain."""

    def path_return_ok(self, path: str, request: Request) -> bool:
        """Return false if cookies should not be returned, given cookie path."""

class DefaultCookiePolicy(CookiePolicy):
    """Implements the standard rules for accepting and returning cookies."""

    rfc2109_as_netscape: bool
    strict_domain: bool
    strict_rfc2965_unverifiable: bool
    strict_ns_unverifiable: bool
    strict_ns_domain: int
    strict_ns_set_initial_dollar: bool
    strict_ns_set_path: bool
    DomainStrictNoDots: ClassVar[int]
    DomainStrictNonDomain: ClassVar[int]
    DomainRFC2965Match: ClassVar[int]
    DomainLiberal: ClassVar[int]
    DomainStrict: ClassVar[int]
    def __init__(
        self,
        blocked_domains: Sequence[str] | None = None,
        allowed_domains: Sequence[str] | None = None,
        netscape: bool = True,
        rfc2965: bool = False,
        rfc2109_as_netscape: bool | None = None,
        hide_cookie2: bool = False,
        strict_domain: bool = False,
        strict_rfc2965_unverifiable: bool = True,
        strict_ns_unverifiable: bool = False,
        strict_ns_domain: int = 0,
        strict_ns_set_initial_dollar: bool = False,
        strict_ns_set_path: bool = False,
        secure_protocols: Sequence[str] = ("https", "wss"),
    ) -> None:
        """Constructor arguments should be passed as keyword arguments only."""

    def blocked_domains(self) -> tuple[str, ...]:
        """Return the sequence of blocked domains (as a tuple)."""

    def set_blocked_domains(self, blocked_domains: Sequence[str]) -> None:
        """Set the sequence of blocked domains."""

    def is_blocked(self, domain: str) -> bool: ...
    def allowed_domains(self) -> tuple[str, ...] | None:
        """Return None, or the sequence of allowed domains (as a tuple)."""

    def set_allowed_domains(self, allowed_domains: Sequence[str] | None) -> None:
        """Set the sequence of allowed domains, or None."""

    def is_not_allowed(self, domain: str) -> bool: ...
    def set_ok_version(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def set_ok_verifiability(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def set_ok_name(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def set_ok_path(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def set_ok_domain(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def set_ok_port(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_version(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_verifiability(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_secure(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_expires(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_port(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented
    def return_ok_domain(self, cookie: Cookie, request: Request) -> bool: ...  # undocumented

class Cookie:
    """HTTP Cookie.

    This class represents both Netscape and RFC 2965 cookies.

    This is deliberately a very simple class.  It just holds attributes.  It's
    possible to construct Cookie instances that don't comply with the cookie
    standards.  CookieJar.make_cookies is the factory function for Cookie
    objects -- it deals with cookie parsing, supplying defaults, and
    normalising to the representation used in this class.  CookiePolicy is
    responsible for checking them to see whether they should be accepted from
    and returned to the server.

    Note that the port may be present in the headers, but unspecified ("Port"
    rather than"Port=80", for example); if this is the case, port is None.

    """

    version: int | None
    name: str
    value: str | None
    port: str | None
    path: str
    path_specified: bool
    secure: bool
    expires: int | None
    discard: bool
    comment: str | None
    comment_url: str | None
    rfc2109: bool
    port_specified: bool
    domain: str  # undocumented
    domain_specified: bool
    domain_initial_dot: bool
    def __init__(
        self,
        version: int | None,
        name: str,
        value: str | None,  # undocumented
        port: str | None,
        port_specified: bool,
        domain: str,
        domain_specified: bool,
        domain_initial_dot: bool,
        path: str,
        path_specified: bool,
        secure: bool,
        expires: int | None,
        discard: bool,
        comment: str | None,
        comment_url: str | None,
        rest: dict[str, str],
        rfc2109: bool = False,
    ) -> None: ...
    def has_nonstandard_attr(self, name: str) -> bool: ...
    @overload
    def get_nonstandard_attr(self, name: str) -> str | None: ...
    @overload
    def get_nonstandard_attr(self, name: str, default: _T) -> str | _T: ...
    def set_nonstandard_attr(self, name: str, value: str) -> None: ...
    def is_expired(self, now: int | None = None) -> bool: ...
