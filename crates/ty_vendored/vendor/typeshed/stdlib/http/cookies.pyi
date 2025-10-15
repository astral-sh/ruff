"""
Here's a sample session to show how to use this module.
At the moment, this is the only documentation.

The Basics
----------

Importing is easy...

   >>> from http import cookies

Most of the time you start by creating a cookie.

   >>> C = cookies.SimpleCookie()

Once you've created your Cookie, you can add values just as if it were
a dictionary.

   >>> C = cookies.SimpleCookie()
   >>> C["fig"] = "newton"
   >>> C["sugar"] = "wafer"
   >>> C.output()
   'Set-Cookie: fig=newton\\r\\nSet-Cookie: sugar=wafer'

Notice that the printable representation of a Cookie is the
appropriate format for a Set-Cookie: header.  This is the
default behavior.  You can change the header and printed
attributes by using the .output() function

   >>> C = cookies.SimpleCookie()
   >>> C["rocky"] = "road"
   >>> C["rocky"]["path"] = "/cookie"
   >>> print(C.output(header="Cookie:"))
   Cookie: rocky=road; Path=/cookie
   >>> print(C.output(attrs=[], header="Cookie:"))
   Cookie: rocky=road

The load() method of a Cookie extracts cookies from a string.  In a
CGI script, you would use this method to extract the cookies from the
HTTP_COOKIE environment variable.

   >>> C = cookies.SimpleCookie()
   >>> C.load("chips=ahoy; vienna=finger")
   >>> C.output()
   'Set-Cookie: chips=ahoy\\r\\nSet-Cookie: vienna=finger'

The load() method is darn-tootin smart about identifying cookies
within a string.  Escaped quotation marks, nested semicolons, and other
such trickeries do not confuse it.

   >>> C = cookies.SimpleCookie()
   >>> C.load('keebler="E=everybody; L=\\\\"Loves\\\\"; fudge=\\\\012;";')
   >>> print(C)
   Set-Cookie: keebler="E=everybody; L=\\"Loves\\"; fudge=\\012;"

Each element of the Cookie also supports all of the RFC 2109
Cookie attributes.  Here's an example which sets the Path
attribute.

   >>> C = cookies.SimpleCookie()
   >>> C["oreo"] = "doublestuff"
   >>> C["oreo"]["path"] = "/"
   >>> print(C)
   Set-Cookie: oreo=doublestuff; Path=/

Each dictionary element has a 'value' attribute, which gives you
back the value associated with the key.

   >>> C = cookies.SimpleCookie()
   >>> C["twix"] = "none for you"
   >>> C["twix"].value
   'none for you'

The SimpleCookie expects that all values should be standard strings.
Just to be sure, SimpleCookie invokes the str() builtin to convert
the value to a string, when the values are set dictionary-style.

   >>> C = cookies.SimpleCookie()
   >>> C["number"] = 7
   >>> C["string"] = "seven"
   >>> C["number"].value
   '7'
   >>> C["string"].value
   'seven'
   >>> C.output()
   'Set-Cookie: number=7\\r\\nSet-Cookie: string=seven'

Finis.
"""

from collections.abc import Iterable, Mapping
from types import GenericAlias
from typing import Any, Generic, TypeVar, overload
from typing_extensions import TypeAlias

__all__ = ["CookieError", "BaseCookie", "SimpleCookie"]

_DataType: TypeAlias = str | Mapping[str, str | Morsel[Any]]
_T = TypeVar("_T")

@overload
def _quote(str: None) -> None:
    """Quote a string for use in a cookie header.

    If the string does not need to be double-quoted, then just return the
    string.  Otherwise, surround the string in doublequotes and quote
    (with a \\) special characters.
    """

@overload
def _quote(str: str) -> str: ...
@overload
def _unquote(str: None) -> None: ...
@overload
def _unquote(str: str) -> str: ...

class CookieError(Exception): ...

class Morsel(dict[str, Any], Generic[_T]):
    """A class to hold ONE (key, value) pair.

    In a cookie, each such pair may have several attributes, so this class is
    used to keep the attributes associated with the appropriate key,value pair.
    This class also includes a coded_value attribute, which is used to hold
    the network representation of the value.
    """

    @property
    def value(self) -> str: ...
    @property
    def coded_value(self) -> _T: ...
    @property
    def key(self) -> str: ...
    def __init__(self) -> None: ...
    def set(self, key: str, val: str, coded_val: _T) -> None: ...
    def setdefault(self, key: str, val: str | None = None) -> str: ...
    # The dict update can also get a keywords argument so this is incompatible
    @overload  # type: ignore[override]
    def update(self, values: Mapping[str, str]) -> None: ...
    @overload
    def update(self, values: Iterable[tuple[str, str]]) -> None: ...
    def isReservedKey(self, K: str) -> bool: ...
    def output(self, attrs: list[str] | None = None, header: str = "Set-Cookie:") -> str: ...
    __str__ = output
    def js_output(self, attrs: list[str] | None = None) -> str: ...
    def OutputString(self, attrs: list[str] | None = None) -> str: ...
    def __eq__(self, morsel: object) -> bool: ...
    def __setitem__(self, K: str, V: Any) -> None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class BaseCookie(dict[str, Morsel[_T]], Generic[_T]):
    """A container class for a set of Morsels."""

    def __init__(self, input: _DataType | None = None) -> None: ...
    def value_decode(self, val: str) -> tuple[_T, str]:
        """real_value, coded_value = value_decode(STRING)
        Called prior to setting a cookie's value from the network
        representation.  The VALUE is the value read from HTTP
        header.
        Override this function to modify the behavior of cookies.
        """

    def value_encode(self, val: _T) -> tuple[_T, str]:
        """real_value, coded_value = value_encode(VALUE)
        Called prior to setting a cookie's value from the dictionary
        representation.  The VALUE is the value being assigned.
        Override this function to modify the behavior of cookies.
        """

    def output(self, attrs: list[str] | None = None, header: str = "Set-Cookie:", sep: str = "\r\n") -> str:
        """Return a string suitable for HTTP."""
    __str__ = output
    def js_output(self, attrs: list[str] | None = None) -> str:
        """Return a string suitable for JavaScript."""

    def load(self, rawdata: _DataType) -> None:
        """Load cookies from a string (presumably HTTP_COOKIE) or
        from a dictionary.  Loading cookies from a dictionary 'd'
        is equivalent to calling:
            map(Cookie.__setitem__, d.keys(), d.values())
        """

    def __setitem__(self, key: str, value: str | Morsel[_T]) -> None:
        """Dictionary style assignment."""

class SimpleCookie(BaseCookie[str]):
    """
    SimpleCookie supports strings as cookie values.  When setting
    the value using the dictionary assignment notation, SimpleCookie
    calls the builtin str() to convert the value to a string.  Values
    received from HTTP are kept as strings.
    """
