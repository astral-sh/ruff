from collections.abc import Iterable, Mapping
from getopt import _SliceableT, _StrSequenceT_co
from re import Pattern
from typing import Any, Final, overload
from typing_extensions import TypeAlias

_Option: TypeAlias = tuple[str, str | None, str]

longopt_pat: Final = r"[a-zA-Z](?:[a-zA-Z0-9-]*)"
longopt_re: Final[Pattern[str]]
neg_alias_re: Final[Pattern[str]]
longopt_xlate: Final[dict[int, int]]

class FancyGetopt:
    def __init__(self, option_table: list[_Option] | None = None) -> None: ...
    # TODO: kinda wrong, `getopt(object=object())` is invalid
    @overload
    def getopt(
        self, args: _SliceableT[_StrSequenceT_co] | None = None, object: None = None
    ) -> tuple[_StrSequenceT_co, OptionDummy]: ...
    @overload
    def getopt(
        self, args: _SliceableT[_StrSequenceT_co] | None, object: Any
    ) -> _StrSequenceT_co: ...  # object is an arbitrary non-slotted object
    def get_option_order(self) -> list[tuple[str, str]]: ...
    def generate_help(self, header: str | None = None) -> list[str]: ...

# Same note as FancyGetopt.getopt
@overload
def fancy_getopt(
    options: list[_Option], negative_opt: Mapping[_Option, _Option], object: None, args: _SliceableT[_StrSequenceT_co] | None
) -> tuple[_StrSequenceT_co, OptionDummy]: ...
@overload
def fancy_getopt(
    options: list[_Option], negative_opt: Mapping[_Option, _Option], object: Any, args: _SliceableT[_StrSequenceT_co] | None
) -> _StrSequenceT_co: ...

WS_TRANS: Final[dict[int, str]]

def wrap_text(text: str, width: int) -> list[str]: ...
def translate_longopt(opt: str) -> str: ...

class OptionDummy:
    def __init__(self, options: Iterable[str] = []) -> None: ...
