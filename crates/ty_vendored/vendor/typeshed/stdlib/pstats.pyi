import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterable
from cProfile import Profile as _cProfile
from dataclasses import dataclass
from profile import Profile
from typing import IO, Any, Literal, overload
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 11):
    from enum import StrEnum
else:
    from enum import Enum

__all__ = ["Stats", "SortKey", "FunctionProfile", "StatsProfile"]

_Selector: TypeAlias = str | float | int

if sys.version_info >= (3, 11):
    class SortKey(StrEnum):
        CALLS = "calls"
        CUMULATIVE = "cumulative"
        FILENAME = "filename"
        LINE = "line"
        NAME = "name"
        NFL = "nfl"
        PCALLS = "pcalls"
        STDNAME = "stdname"
        TIME = "time"

else:
    class SortKey(str, Enum):
        CALLS = "calls"
        CUMULATIVE = "cumulative"
        FILENAME = "filename"
        LINE = "line"
        NAME = "name"
        NFL = "nfl"
        PCALLS = "pcalls"
        STDNAME = "stdname"
        TIME = "time"

@dataclass(unsafe_hash=True)
class FunctionProfile:
    ncalls: str
    tottime: float
    percall_tottime: float
    cumtime: float
    percall_cumtime: float
    file_name: str
    line_number: int

@dataclass(unsafe_hash=True)
class StatsProfile:
    total_tt: float
    func_profiles: dict[str, FunctionProfile]

_SortArgDict: TypeAlias = dict[str, tuple[tuple[tuple[int, int], ...], str]]

class Stats:
    sort_arg_dict_default: _SortArgDict
    def __init__(
        self,
        arg: None | str | Profile | _cProfile = ...,
        /,
        *args: None | str | Profile | _cProfile | Self,
        stream: IO[Any] | None = None,
    ) -> None: ...
    def init(self, arg: None | str | Profile | _cProfile) -> None: ...
    def load_stats(self, arg: None | str | Profile | _cProfile) -> None: ...
    def get_top_level_stats(self) -> None: ...
    def add(self, *arg_list: None | str | Profile | _cProfile | Self) -> Self: ...
    def dump_stats(self, filename: StrOrBytesPath) -> None: ...
    def get_sort_arg_defs(self) -> _SortArgDict: ...
    @overload
    def sort_stats(self, field: Literal[-1, 0, 1, 2]) -> Self: ...
    @overload
    def sort_stats(self, *field: str) -> Self: ...
    def reverse_order(self) -> Self: ...
    def strip_dirs(self) -> Self: ...
    def calc_callees(self) -> None: ...
    def eval_print_amount(self, sel: _Selector, list: list[str], msg: str) -> tuple[list[str], str]: ...
    def get_stats_profile(self) -> StatsProfile: ...
    def get_print_list(self, sel_list: Iterable[_Selector]) -> tuple[int, list[str]]: ...
    def print_stats(self, *amount: _Selector) -> Self: ...
    def print_callees(self, *amount: _Selector) -> Self: ...
    def print_callers(self, *amount: _Selector) -> Self: ...
    def print_call_heading(self, name_size: int, column_title: str) -> None: ...
    def print_call_line(self, name_size: int, source: str, call_dict: dict[str, Any], arrow: str = "->") -> None: ...
    def print_title(self) -> None: ...
    def print_line(self, func: str) -> None: ...
