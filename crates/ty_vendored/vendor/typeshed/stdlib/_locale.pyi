import sys
from _typeshed import StrPath
from typing import Final, Literal, TypedDict, type_check_only

@type_check_only
class _LocaleConv(TypedDict):
    decimal_point: str
    grouping: list[int]
    thousands_sep: str
    int_curr_symbol: str
    currency_symbol: str
    p_cs_precedes: Literal[0, 1, 127]
    n_cs_precedes: Literal[0, 1, 127]
    p_sep_by_space: Literal[0, 1, 127]
    n_sep_by_space: Literal[0, 1, 127]
    mon_decimal_point: str
    frac_digits: int
    int_frac_digits: int
    mon_thousands_sep: str
    mon_grouping: list[int]
    positive_sign: str
    negative_sign: str
    p_sign_posn: Literal[0, 1, 2, 3, 4, 127]
    n_sign_posn: Literal[0, 1, 2, 3, 4, 127]

LC_CTYPE: Final[int]
LC_COLLATE: Final[int]
LC_TIME: Final[int]
LC_MONETARY: Final[int]
LC_NUMERIC: Final[int]
LC_ALL: Final[int]
CHAR_MAX: Final = 127

def setlocale(category: int, locale: str | None = None, /) -> str: ...
def localeconv() -> _LocaleConv: ...

if sys.version_info >= (3, 11):
    def getencoding() -> str: ...

def strcoll(os1: str, os2: str, /) -> int: ...
def strxfrm(string: str, /) -> str: ...

# native gettext functions
# https://docs.python.org/3/library/locale.html#access-to-message-catalogs
# https://github.com/python/cpython/blob/f4c03484da59049eb62a9bf7777b963e2267d187/Modules/_localemodule.c#L626
if sys.platform != "win32":
    LC_MESSAGES: int

    ABDAY_1: Final[int]
    ABDAY_2: Final[int]
    ABDAY_3: Final[int]
    ABDAY_4: Final[int]
    ABDAY_5: Final[int]
    ABDAY_6: Final[int]
    ABDAY_7: Final[int]

    ABMON_1: Final[int]
    ABMON_2: Final[int]
    ABMON_3: Final[int]
    ABMON_4: Final[int]
    ABMON_5: Final[int]
    ABMON_6: Final[int]
    ABMON_7: Final[int]
    ABMON_8: Final[int]
    ABMON_9: Final[int]
    ABMON_10: Final[int]
    ABMON_11: Final[int]
    ABMON_12: Final[int]

    DAY_1: Final[int]
    DAY_2: Final[int]
    DAY_3: Final[int]
    DAY_4: Final[int]
    DAY_5: Final[int]
    DAY_6: Final[int]
    DAY_7: Final[int]

    ERA: Final[int]
    ERA_D_T_FMT: Final[int]
    ERA_D_FMT: Final[int]
    ERA_T_FMT: Final[int]

    MON_1: Final[int]
    MON_2: Final[int]
    MON_3: Final[int]
    MON_4: Final[int]
    MON_5: Final[int]
    MON_6: Final[int]
    MON_7: Final[int]
    MON_8: Final[int]
    MON_9: Final[int]
    MON_10: Final[int]
    MON_11: Final[int]
    MON_12: Final[int]

    CODESET: Final[int]
    D_T_FMT: Final[int]
    D_FMT: Final[int]
    T_FMT: Final[int]
    T_FMT_AMPM: Final[int]
    AM_STR: Final[int]
    PM_STR: Final[int]

    RADIXCHAR: Final[int]
    THOUSEP: Final[int]
    YESEXPR: Final[int]
    NOEXPR: Final[int]
    CRNCYSTR: Final[int]
    ALT_DIGITS: Final[int]

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
