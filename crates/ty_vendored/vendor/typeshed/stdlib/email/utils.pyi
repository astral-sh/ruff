"""Miscellaneous utilities."""

import datetime
import sys
from _typeshed import Unused
from collections.abc import Iterable
from email import _ParamType
from email.charset import Charset
from typing import overload
from typing_extensions import TypeAlias, deprecated

__all__ = [
    "collapse_rfc2231_value",
    "decode_params",
    "decode_rfc2231",
    "encode_rfc2231",
    "formataddr",
    "formatdate",
    "format_datetime",
    "getaddresses",
    "make_msgid",
    "mktime_tz",
    "parseaddr",
    "parsedate",
    "parsedate_tz",
    "parsedate_to_datetime",
    "unquote",
]

_PDTZ: TypeAlias = tuple[int, int, int, int, int, int, int, int, int, int | None]

def quote(str: str) -> str:
    """Prepare string to be used in a quoted string.

    Turns backslash and double quote characters into quoted pairs.  These
    are the only characters that need to be quoted inside a quoted string.
    Does not add the surrounding double quotes.
    """

def unquote(str: str) -> str:
    """Remove quotes from a string."""

# `strict` parameter added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
def parseaddr(addr: str | list[str], *, strict: bool = True) -> tuple[str, str]:
    """
    Parse addr into its constituent realname and email address parts.

    Return a tuple of realname and email address, unless the parse fails, in
    which case return a 2-tuple of ('', '').

    If strict is True, use a strict parser which rejects malformed inputs.
    """

def formataddr(pair: tuple[str | None, str], charset: str | Charset = "utf-8") -> str:
    """The inverse of parseaddr(), this takes a 2-tuple of the form
    (realname, email_address) and returns the string value suitable
    for an RFC 2822 From, To or Cc header.

    If the first element of pair is false, then the second element is
    returned unmodified.

    The optional charset is the character set that is used to encode
    realname in case realname is not ASCII safe.  Can be an instance of str or
    a Charset-like object which has a header_encode method.  Default is
    'utf-8'.
    """

# `strict` parameter added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
def getaddresses(fieldvalues: Iterable[str], *, strict: bool = True) -> list[tuple[str, str]]:
    """Return a list of (REALNAME, EMAIL) or ('','') for each fieldvalue.

    When parsing fails for a fieldvalue, a 2-tuple of ('', '') is returned in
    its place.

    If strict is true, use a strict parser which rejects malformed inputs.
    """

@overload
def parsedate(data: None) -> None:
    """Convert a time string to a time tuple."""

@overload
def parsedate(data: str) -> tuple[int, int, int, int, int, int, int, int, int] | None: ...
@overload
def parsedate_tz(data: None) -> None:
    """Convert a date string to a time tuple.

    Accounts for military timezones.
    """

@overload
def parsedate_tz(data: str) -> _PDTZ | None: ...

if sys.version_info >= (3, 10):
    @overload
    def parsedate_to_datetime(data: None) -> None: ...
    @overload
    def parsedate_to_datetime(data: str) -> datetime.datetime: ...

else:
    def parsedate_to_datetime(data: str) -> datetime.datetime: ...

def mktime_tz(data: _PDTZ) -> int:
    """Turn a 10-tuple as returned by parsedate_tz() into a POSIX timestamp."""

def formatdate(timeval: float | None = None, localtime: bool = False, usegmt: bool = False) -> str:
    """Returns a date string as specified by RFC 2822, e.g.:

    Fri, 09 Nov 2001 01:08:47 -0000

    Optional timeval if given is a floating-point time value as accepted by
    gmtime() and localtime(), otherwise the current time is used.

    Optional localtime is a flag that when True, interprets timeval, and
    returns a date relative to the local timezone instead of UTC, properly
    taking daylight savings time into account.

    Optional argument usegmt means that the timezone is written out as
    an ascii string, not numeric one (so "GMT" instead of "+0000"). This
    is needed for HTTP, and is only used when localtime==False.
    """

def format_datetime(dt: datetime.datetime, usegmt: bool = False) -> str:
    """Turn a datetime into a date string as specified in RFC 2822.

    If usegmt is True, dt must be an aware datetime with an offset of zero.  In
    this case 'GMT' will be rendered instead of the normal +0000 required by
    RFC2822.  This is to support HTTP headers involving date stamps.
    """

if sys.version_info >= (3, 14):
    def localtime(dt: datetime.datetime | None = None) -> datetime.datetime:
        """Return local time as an aware datetime object.

        If called without arguments, return current time.  Otherwise *dt*
        argument should be a datetime instance, and it is converted to the
        local time zone according to the system time zone database.  If *dt* is
        naive (that is, dt.tzinfo is None), it is assumed to be in local time.

        """

elif sys.version_info >= (3, 12):
    @overload
    def localtime(dt: datetime.datetime | None = None) -> datetime.datetime:
        """Return local time as an aware datetime object.

        If called without arguments, return current time.  Otherwise *dt*
        argument should be a datetime instance, and it is converted to the
        local time zone according to the system time zone database.  If *dt* is
        naive (that is, dt.tzinfo is None), it is assumed to be in local time.
        The isdst parameter is ignored.

        """

    @overload
    @deprecated("The `isdst` parameter does nothing and will be removed in Python 3.14.")
    def localtime(dt: datetime.datetime | None = None, isdst: Unused = None) -> datetime.datetime: ...

else:
    def localtime(dt: datetime.datetime | None = None, isdst: int = -1) -> datetime.datetime:
        """Return local time as an aware datetime object.

        If called without arguments, return current time.  Otherwise *dt*
        argument should be a datetime instance, and it is converted to the
        local time zone according to the system time zone database.  If *dt* is
        naive (that is, dt.tzinfo is None), it is assumed to be in local time.
        In this case, a positive or zero value for *isdst* causes localtime to
        presume initially that summer time (for example, Daylight Saving Time)
        is or is not (respectively) in effect for the specified time.  A
        negative value for *isdst* causes the localtime() function to attempt
        to divine whether summer time is in effect for the specified time.

        """

def make_msgid(idstring: str | None = None, domain: str | None = None) -> str:
    """Returns a string suitable for RFC 2822 compliant Message-ID, e.g:

    <142480216486.20800.16526388040877946887@nightshade.la.mastaler.com>

    Optional idstring if given is a string used to strengthen the
    uniqueness of the message id.  Optional domain if given provides the
    portion of the message id after the '@'.  It defaults to the locally
    defined hostname.
    """

def decode_rfc2231(s: str) -> tuple[str | None, str | None, str]:  # May return list[str]. See issue #10431 for details.
    """Decode string according to RFC 2231"""

def encode_rfc2231(s: str, charset: str | None = None, language: str | None = None) -> str:
    """Encode string according to RFC 2231.

    If neither charset nor language is given, then s is returned as-is.  If
    charset is given but not language, the string is encoded using the empty
    string for language.
    """

def collapse_rfc2231_value(value: _ParamType, errors: str = "replace", fallback_charset: str = "us-ascii") -> str: ...
def decode_params(params: list[tuple[str, str]]) -> list[tuple[str, _ParamType]]:
    """Decode parameters list according to RFC 2231.

    params is a sequence of 2-tuples containing (param name, string value).
    """
