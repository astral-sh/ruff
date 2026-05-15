import io
import sys
from _typeshed import StrPath
from collections.abc import Callable, Container, Iterable, Sequence
from typing import Any, Final, Literal, Protocol, TypeVar, overload, type_check_only
from typing_extensions import deprecated

__all__ = [
    "NullTranslations",
    "GNUTranslations",
    "Catalog",
    "find",
    "translation",
    "install",
    "textdomain",
    "bindtextdomain",
    "dgettext",
    "dngettext",
    "gettext",
    "ngettext",
    "dnpgettext",
    "dpgettext",
    "npgettext",
    "pgettext",
]

if sys.version_info < (3, 11):
    __all__ += ["bind_textdomain_codeset", "ldgettext", "ldngettext", "lgettext", "lngettext"]

@type_check_only
class _TranslationsReader(Protocol):
    def read(self) -> bytes: ...
    # optional:
    # name: str

class NullTranslations:
    def __init__(self, fp: _TranslationsReader | None = None) -> None: ...
    def _parse(self, fp: _TranslationsReader) -> None: ...
    def add_fallback(self, fallback: NullTranslations) -> None: ...
    def gettext(self, message: str) -> str: ...
    def ngettext(self, msgid1: str, msgid2: str, n: int) -> str: ...
    def pgettext(self, context: str, message: str) -> str: ...
    def npgettext(self, context: str, msgid1: str, msgid2: str, n: int) -> str: ...
    def info(self) -> dict[str, str]: ...
    def charset(self) -> str | None: ...
    if sys.version_info < (3, 11):
        @deprecated("Deprecated since Python 3.8; removed in Python 3.11.")
        def output_charset(self) -> str | None: ...
        @deprecated("Deprecated since Python 3.8; removed in Python 3.11.")
        def set_output_charset(self, charset: str) -> None: ...
        @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `gettext()` instead.")
        def lgettext(self, message: str) -> str: ...
        @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `ngettext()` instead.")
        def lngettext(self, msgid1: str, msgid2: str, n: int) -> str: ...

    def install(self, names: Container[str] | None = None) -> None: ...

class GNUTranslations(NullTranslations):
    LE_MAGIC: Final[int]
    BE_MAGIC: Final[int]
    CONTEXT: str
    VERSIONS: Sequence[int]

@overload
def find(
    domain: str, localedir: StrPath | None = None, languages: Iterable[str] | None = None, all: Literal[False] = False
) -> str | None: ...
@overload
def find(
    domain: str, localedir: StrPath | None = None, languages: Iterable[str] | None = None, *, all: Literal[True]
) -> list[str]: ...
@overload
def find(domain: str, localedir: StrPath | None, languages: Iterable[str] | None, all: Literal[True]) -> list[str]: ...
@overload
def find(domain: str, localedir: StrPath | None = None, languages: Iterable[str] | None = None, all: bool = False) -> Any: ...

_NullTranslationsT = TypeVar("_NullTranslationsT", bound=NullTranslations)

if sys.version_info >= (3, 11):
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        class_: None = None,
        fallback: Literal[False] = False,
    ) -> GNUTranslations: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        *,
        class_: Callable[[io.BufferedReader], _NullTranslationsT],
        fallback: Literal[False] = False,
    ) -> _NullTranslationsT: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None,
        languages: Iterable[str] | None,
        class_: Callable[[io.BufferedReader], _NullTranslationsT],
        fallback: Literal[False] = False,
    ) -> _NullTranslationsT: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        class_: Callable[[io.BufferedReader], NullTranslations] | None = None,
        fallback: bool = False,
    ) -> NullTranslations: ...
    def install(domain: str, localedir: StrPath | None = None, *, names: Container[str] | None = None) -> None: ...

else:
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        class_: None = None,
        fallback: Literal[False] = False,
        codeset: str | None = ...,
    ) -> GNUTranslations: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        *,
        class_: Callable[[io.BufferedReader], _NullTranslationsT],
        fallback: Literal[False] = False,
        codeset: str | None = ...,
    ) -> _NullTranslationsT: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None,
        languages: Iterable[str] | None,
        class_: Callable[[io.BufferedReader], _NullTranslationsT],
        fallback: Literal[False] = False,
        codeset: str | None = ...,
    ) -> _NullTranslationsT: ...
    @overload
    def translation(
        domain: str,
        localedir: StrPath | None = None,
        languages: Iterable[str] | None = None,
        class_: Callable[[io.BufferedReader], NullTranslations] | None = None,
        fallback: bool = False,
        codeset: str | None = ...,
    ) -> NullTranslations: ...
    @overload
    def install(domain: str, localedir: StrPath | None = None, names: Container[str] | None = None) -> None: ...
    @overload
    @deprecated("The `codeset` parameter is deprecated since Python 3.8; removed in Python 3.11.")
    def install(domain: str, localedir: StrPath | None, codeset: str | None, /, names: Container[str] | None = None) -> None: ...
    @overload
    @deprecated("The `codeset` parameter is deprecated since Python 3.8; removed in Python 3.11.")
    def install(
        domain: str, localedir: StrPath | None = None, *, codeset: str | None, names: Container[str] | None = None
    ) -> None: ...

def textdomain(domain: str | None = None) -> str: ...
def bindtextdomain(domain: str, localedir: StrPath | None = None) -> str: ...
def dgettext(domain: str, message: str) -> str: ...
def dngettext(domain: str, msgid1: str, msgid2: str, n: int) -> str: ...
def gettext(message: str) -> str: ...
def ngettext(msgid1: str, msgid2: str, n: int) -> str: ...
def pgettext(context: str, message: str) -> str: ...
def dpgettext(domain: str, context: str, message: str) -> str: ...
def npgettext(context: str, msgid1: str, msgid2: str, n: int) -> str: ...
def dnpgettext(domain: str, context: str, msgid1: str, msgid2: str, n: int) -> str: ...

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `gettext()` instead.")
    def lgettext(message: str) -> str: ...
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `dgettext()` instead.")
    def ldgettext(domain: str, message: str) -> str: ...
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `ngettext()` instead.")
    def lngettext(msgid1: str, msgid2: str, n: int) -> str: ...
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `dngettext()` instead.")
    def ldngettext(domain: str, msgid1: str, msgid2: str, n: int) -> str: ...
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `bindtextdomain()` instead.")
    def bind_textdomain_codeset(domain: str, codeset: str | None = None) -> str: ...

Catalog = translation

def c2py(plural: str) -> Callable[[int], int]: ...
