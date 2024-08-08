import sys
from _typeshed import StrPath
from collections.abc import Mapping

LC_CTYPE: int
LC_COLLATE: int
LC_TIME: int
LC_MONETARY: int
LC_NUMERIC: int
LC_ALL: int
CHAR_MAX: int

def setlocale(category: int, locale: str | None = None, /) -> str: ...
def localeconv() -> Mapping[str, int | str | list[int]]: ...

if sys.version_info >= (3, 11):
    def getencoding() -> str: ...

def strcoll(os1: str, os2: str, /) -> int: ...
def strxfrm(string: str, /) -> str: ...

# native gettext functions
# https://docs.python.org/3/library/locale.html#access-to-message-catalogs
# https://github.com/python/cpython/blob/f4c03484da59049eb62a9bf7777b963e2267d187/Modules/_localemodule.c#L626
if sys.platform != "win32":
    LC_MESSAGES: int

    ABDAY_1: int
    ABDAY_2: int
    ABDAY_3: int
    ABDAY_4: int
    ABDAY_5: int
    ABDAY_6: int
    ABDAY_7: int

    ABMON_1: int
    ABMON_2: int
    ABMON_3: int
    ABMON_4: int
    ABMON_5: int
    ABMON_6: int
    ABMON_7: int
    ABMON_8: int
    ABMON_9: int
    ABMON_10: int
    ABMON_11: int
    ABMON_12: int

    DAY_1: int
    DAY_2: int
    DAY_3: int
    DAY_4: int
    DAY_5: int
    DAY_6: int
    DAY_7: int

    ERA: int
    ERA_D_T_FMT: int
    ERA_D_FMT: int
    ERA_T_FMT: int

    MON_1: int
    MON_2: int
    MON_3: int
    MON_4: int
    MON_5: int
    MON_6: int
    MON_7: int
    MON_8: int
    MON_9: int
    MON_10: int
    MON_11: int
    MON_12: int

    CODESET: int
    D_T_FMT: int
    D_FMT: int
    T_FMT: int
    T_FMT_AMPM: int
    AM_STR: int
    PM_STR: int

    RADIXCHAR: int
    THOUSEP: int
    YESEXPR: int
    NOEXPR: int
    CRNCYSTR: int
    ALT_DIGITS: int

    def nl_langinfo(key: int, /) -> str: ...

    # This is dependent on `libintl.h` which is a part of `gettext`
    # system dependency. These functions might be missing.
    # But, we always say that they are present.
    def gettext(msg: str, /) -> str: ...
    def dgettext(domain: str | None, msg: str, /) -> str: ...
    def dcgettext(domain: str | None, msg: str, category: int, /) -> str: ...
    def textdomain(domain: str | None, /) -> str: ...
    def bindtextdomain(domain: str, dir: StrPath | None, /) -> str: ...
    def bind_textdomain_codeset(domain: str, codeset: str | None, /) -> str | None: ...
