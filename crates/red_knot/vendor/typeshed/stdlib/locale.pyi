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

def getdefaultlocale(
    envvars: tuple[_str, ...] = ("LC_ALL", "LC_CTYPE", "LANG", "LANGUAGE")
) -> tuple[_str | None, _str | None]: ...
def getlocale(category: int = ...) -> tuple[_str | None, _str | None]: ...
def setlocale(category: int, locale: _str | Iterable[_str | None] | None = None) -> _str: ...
def getpreferredencoding(do_setlocale: bool = True) -> _str: ...
def normalize(localename: _str) -> _str: ...

if sys.version_info < (3, 13):
    def resetlocale(category: int = ...) -> None: ...

if sys.version_info < (3, 12):
    def format(
        percent: _str, value: float | Decimal, grouping: bool = False, monetary: bool = False, *additional: Any
    ) -> _str: ...

def format_string(f: _str, val: Any, grouping: bool = False, monetary: bool = False) -> _str: ...
def currency(val: float | Decimal, symbol: bool = True, grouping: bool = False, international: bool = False) -> _str: ...
def delocalize(string: _str) -> _str: ...
def atof(string: _str, func: Callable[[_str], float] = ...) -> float: ...
def atoi(string: _str) -> int: ...
def str(val: float) -> _str: ...

locale_alias: dict[_str, _str]  # undocumented
locale_encoding_alias: dict[_str, _str]  # undocumented
windows_locale: dict[int, _str]  # undocumented
