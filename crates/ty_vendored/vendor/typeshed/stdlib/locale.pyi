"""Locale support module.

The module provides low-level access to the C lib's locale APIs and adds high
level number formatting APIs as well as a locale aliasing engine to complement
these.

The aliasing engine includes support for many commonly used locale names and
maps them to values suitable for passing to the C lib's setlocale() function. It
also includes default encodings for all supported locale names.

"""

import sys
from _locale import (
    CHAR_MAX as CHAR_MAX,
    LC_ALL as LC_ALL,
    LC_COLLATE as LC_COLLATE,
    LC_CTYPE as LC_CTYPE,
    LC_MONETARY as LC_MONETARY,
    LC_NUMERIC as LC_NUMERIC,
    LC_TIME as LC_TIME,
    localeconv as localeconv,
    strcoll as strcoll,
    strxfrm as strxfrm,
)

# This module defines a function "str()", which is why "str" can't be used
# as a type annotation or type alias.
from builtins import str as _str
from collections.abc import Callable, Iterable
from decimal import Decimal
from typing import Any
from typing_extensions import deprecated

if sys.version_info >= (3, 11):
    from _locale import getencoding as getencoding

# Some parts of the `_locale` module are platform-specific:
if sys.platform != "win32":
    from _locale import (
        ABDAY_1 as ABDAY_1,
        ABDAY_2 as ABDAY_2,
        ABDAY_3 as ABDAY_3,
        ABDAY_4 as ABDAY_4,
        ABDAY_5 as ABDAY_5,
        ABDAY_6 as ABDAY_6,
        ABDAY_7 as ABDAY_7,
        ABMON_1 as ABMON_1,
        ABMON_2 as ABMON_2,
        ABMON_3 as ABMON_3,
        ABMON_4 as ABMON_4,
        ABMON_5 as ABMON_5,
        ABMON_6 as ABMON_6,
        ABMON_7 as ABMON_7,
        ABMON_8 as ABMON_8,
        ABMON_9 as ABMON_9,
        ABMON_10 as ABMON_10,
        ABMON_11 as ABMON_11,
        ABMON_12 as ABMON_12,
        ALT_DIGITS as ALT_DIGITS,
        AM_STR as AM_STR,
        CODESET as CODESET,
        CRNCYSTR as CRNCYSTR,
        D_FMT as D_FMT,
        D_T_FMT as D_T_FMT,
        DAY_1 as DAY_1,
        DAY_2 as DAY_2,
        DAY_3 as DAY_3,
        DAY_4 as DAY_4,
        DAY_5 as DAY_5,
        DAY_6 as DAY_6,
        DAY_7 as DAY_7,
        ERA as ERA,
        ERA_D_FMT as ERA_D_FMT,
        ERA_D_T_FMT as ERA_D_T_FMT,
        ERA_T_FMT as ERA_T_FMT,
        LC_MESSAGES as LC_MESSAGES,
        MON_1 as MON_1,
        MON_2 as MON_2,
        MON_3 as MON_3,
        MON_4 as MON_4,
        MON_5 as MON_5,
        MON_6 as MON_6,
        MON_7 as MON_7,
        MON_8 as MON_8,
        MON_9 as MON_9,
        MON_10 as MON_10,
        MON_11 as MON_11,
        MON_12 as MON_12,
        NOEXPR as NOEXPR,
        PM_STR as PM_STR,
        RADIXCHAR as RADIXCHAR,
        T_FMT as T_FMT,
        T_FMT_AMPM as T_FMT_AMPM,
        THOUSEP as THOUSEP,
        YESEXPR as YESEXPR,
        bind_textdomain_codeset as bind_textdomain_codeset,
        bindtextdomain as bindtextdomain,
        dcgettext as dcgettext,
        dgettext as dgettext,
        gettext as gettext,
        nl_langinfo as nl_langinfo,
        textdomain as textdomain,
    )

__all__ = [
    "getlocale",
    "getdefaultlocale",
    "getpreferredencoding",
    "Error",
    "setlocale",
    "localeconv",
    "strcoll",
    "strxfrm",
    "str",
    "atof",
    "atoi",
    "format_string",
    "currency",
    "normalize",
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_TIME",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_ALL",
    "CHAR_MAX",
]

if sys.version_info >= (3, 11):
    __all__ += ["getencoding"]

if sys.version_info < (3, 12):
    __all__ += ["format"]

if sys.version_info < (3, 13):
    __all__ += ["resetlocale"]

if sys.platform != "win32":
    __all__ += ["LC_MESSAGES"]

class Error(Exception): ...

def getdefaultlocale(envvars: tuple[_str, ...] = ("LC_ALL", "LC_CTYPE", "LANG", "LANGUAGE")) -> tuple[_str | None, _str | None]:
    """Tries to determine the default locale settings and returns
    them as tuple (language code, encoding).

    According to POSIX, a program which has not called
    setlocale(LC_ALL, "") runs using the portable 'C' locale.
    Calling setlocale(LC_ALL, "") lets it use the default locale as
    defined by the LANG variable. Since we don't want to interfere
    with the current locale setting we thus emulate the behavior
    in the way described above.

    To maintain compatibility with other platforms, not only the
    LANG variable is tested, but a list of variables given as
    envvars parameter. The first found to be defined will be
    used. envvars defaults to the search path used in GNU gettext;
    it must always contain the variable name 'LANG'.

    Except for the code 'C', the language code corresponds to RFC
    1766.  code and encoding can be None in case the values cannot
    be determined.

    """

def getlocale(category: int = ...) -> tuple[_str | None, _str | None]:
    """Returns the current setting for the given locale category as
    tuple (language code, encoding).

    category may be one of the LC_* value except LC_ALL. It
    defaults to LC_CTYPE.

    Except for the code 'C', the language code corresponds to RFC
    1766.  code and encoding can be None in case the values cannot
    be determined.

    """

def setlocale(category: int, locale: _str | Iterable[_str | None] | None = None) -> _str:
    """Set the locale for the given category.  The locale can be
    a string, an iterable of two strings (language code and encoding),
    or None.

    Iterables are converted to strings using the locale aliasing
    engine.  Locale strings are passed directly to the C lib.

    category may be given as one of the LC_* values.

    """

def getpreferredencoding(do_setlocale: bool = True) -> _str:
    """Return the charset that the user is likely using,
    according to the system configuration.
    """

def normalize(localename: _str) -> _str:
    """Returns a normalized locale code for the given locale
    name.

    The returned locale code is formatted for use with
    setlocale().

    If normalization fails, the original name is returned
    unchanged.

    If the given encoding is not known, the function defaults to
    the default encoding for the locale code just like setlocale()
    does.

    """

if sys.version_info < (3, 13):
    if sys.version_info >= (3, 11):
        @deprecated("Deprecated since Python 3.11; removed in Python 3.13. Use `locale.setlocale(locale.LC_ALL, '')` instead.")
        def resetlocale(category: int = ...) -> None:
            """Sets the locale for category to the default setting.

            The default setting is determined by calling
            getdefaultlocale(). category defaults to LC_ALL.

            """
    else:
        def resetlocale(category: int = ...) -> None:
            """Sets the locale for category to the default setting.

            The default setting is determined by calling
            getdefaultlocale(). category defaults to LC_ALL.

            """

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.7; removed in Python 3.12. Use `locale.format_string()` instead.")
    def format(percent: _str, value: float | Decimal, grouping: bool = False, monetary: bool = False, *additional: Any) -> _str:
        """Deprecated, use format_string instead."""

def format_string(f: _str, val: Any, grouping: bool = False, monetary: bool = False) -> _str:
    """Formats a string in the same way that the % formatting would use,
    but takes the current locale into account.

    Grouping is applied if the third parameter is true.
    Conversion uses monetary thousands separator and grouping strings if
    forth parameter monetary is true.
    """

def currency(val: float | Decimal, symbol: bool = True, grouping: bool = False, international: bool = False) -> _str:
    """Formats val according to the currency settings
    in the current locale.
    """

def delocalize(string: _str) -> _str:
    """Parses a string as a normalized number according to the locale settings."""

if sys.version_info >= (3, 10):
    def localize(string: _str, grouping: bool = False, monetary: bool = False) -> _str:
        """Parses a string as locale number according to the locale settings."""

def atof(string: _str, func: Callable[[_str], float] = ...) -> float:
    """Parses a string as a float according to the locale settings."""

def atoi(string: _str) -> int:
    """Converts a string to an integer according to the locale settings."""

def str(val: float) -> _str:
    """Convert float to string, taking the locale into account."""

locale_alias: dict[_str, _str]  # undocumented
locale_encoding_alias: dict[_str, _str]  # undocumented
windows_locale: dict[int, _str]  # undocumented
