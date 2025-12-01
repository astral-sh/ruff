import sys
from _typeshed import StrOrLiteralStr
from collections.abc import Iterable, Mapping, Sequence
from re import Pattern, RegexFlag
from typing import Any, ClassVar, Final, overload
from typing_extensions import LiteralString

__all__ = [
    "ascii_letters",
    "ascii_lowercase",
    "ascii_uppercase",
    "capwords",
    "digits",
    "hexdigits",
    "octdigits",
    "printable",
    "punctuation",
    "whitespace",
    "Formatter",
    "Template",
]

whitespace: Final = " \t\n\r\v\f"
ascii_lowercase: Final = "abcdefghijklmnopqrstuvwxyz"
ascii_uppercase: Final = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
ascii_letters: Final[LiteralString]  # string too long
digits: Final = "0123456789"
hexdigits: Final = "0123456789abcdefABCDEF"
octdigits: Final = "01234567"
punctuation: Final = r"""!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"""
printable: Final[LiteralString]  # string too long

def capwords(s: StrOrLiteralStr, sep: StrOrLiteralStr | None = None) -> StrOrLiteralStr: ...

class Template:
    template: str
    delimiter: ClassVar[str]
    idpattern: ClassVar[str]
    braceidpattern: ClassVar[str | None]
    if sys.version_info >= (3, 14):
        flags: ClassVar[RegexFlag | None]
    else:
        flags: ClassVar[RegexFlag]
    pattern: ClassVar[Pattern[str]]
    def __init__(self, template: str) -> None: ...
    def substitute(self, mapping: Mapping[str, object] = {}, /, **kwds: object) -> str: ...
    def safe_substitute(self, mapping: Mapping[str, object] = {}, /, **kwds: object) -> str: ...
    if sys.version_info >= (3, 11):
        def get_identifiers(self) -> list[str]: ...
        def is_valid(self) -> bool: ...

class Formatter:
    @overload
    def format(self, format_string: LiteralString, /, *args: LiteralString, **kwargs: LiteralString) -> LiteralString: ...
    @overload
    def format(self, format_string: str, /, *args: Any, **kwargs: Any) -> str: ...
    @overload
    def vformat(
        self, format_string: LiteralString, args: Sequence[LiteralString], kwargs: Mapping[LiteralString, LiteralString]
    ) -> LiteralString: ...
    @overload
    def vformat(self, format_string: str, args: Sequence[Any], kwargs: Mapping[str, Any]) -> str: ...
    def _vformat(  # undocumented
        self,
        format_string: str,
        args: Sequence[Any],
        kwargs: Mapping[str, Any],
        used_args: set[int | str],
        recursion_depth: int,
        auto_arg_index: int = 0,
    ) -> tuple[str, int]: ...
    def parse(
        self, format_string: StrOrLiteralStr
    ) -> Iterable[tuple[StrOrLiteralStr, StrOrLiteralStr | None, StrOrLiteralStr | None, StrOrLiteralStr | None]]: ...
    def get_field(self, field_name: str, args: Sequence[Any], kwargs: Mapping[str, Any]) -> Any: ...
    def get_value(self, key: int | str, args: Sequence[Any], kwargs: Mapping[str, Any]) -> Any: ...
    def check_unused_args(self, used_args: set[int | str], args: Sequence[Any], kwargs: Mapping[str, Any]) -> None: ...
    def format_field(self, value: Any, format_spec: str) -> Any: ...
    def convert_field(self, value: Any, conversion: str | None) -> Any: ...
