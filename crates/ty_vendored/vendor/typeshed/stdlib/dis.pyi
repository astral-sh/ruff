import sys
import types
from collections.abc import Callable, Iterator
from opcode import *  # `dis` re-exports it as a part of public API
from typing import IO, Any, NamedTuple
from typing_extensions import Self, TypeAlias

__all__ = [
    "code_info",
    "dis",
    "disassemble",
    "distb",
    "disco",
    "findlinestarts",
    "findlabels",
    "show_code",
    "get_instructions",
    "Instruction",
    "Bytecode",
    "cmp_op",
    "hasconst",
    "hasname",
    "hasjrel",
    "hasjabs",
    "haslocal",
    "hascompare",
    "hasfree",
    "opname",
    "opmap",
    "HAVE_ARGUMENT",
    "EXTENDED_ARG",
    "stack_effect",
]
if sys.version_info >= (3, 13):
    __all__ += ["hasjump"]

if sys.version_info >= (3, 12):
    __all__ += ["hasarg", "hasexc"]
else:
    __all__ += ["hasnargs"]

# Strictly this should not have to include Callable, but mypy doesn't use FunctionType
# for functions (python/mypy#3171)
_HaveCodeType: TypeAlias = types.MethodType | types.FunctionType | types.CodeType | type | Callable[..., Any]

if sys.version_info >= (3, 11):
    class Positions(NamedTuple):
        lineno: int | None = None
        end_lineno: int | None = None
        col_offset: int | None = None
        end_col_offset: int | None = None

if sys.version_info >= (3, 13):
    class _Instruction(NamedTuple):
        opname: str
        opcode: int
        arg: int | None
        argval: Any
        argrepr: str
        offset: int
        start_offset: int
        starts_line: bool
        line_number: int | None
        label: int | None = None
        positions: Positions | None = None
        cache_info: list[tuple[str, int, Any]] | None = None

elif sys.version_info >= (3, 11):
    class _Instruction(NamedTuple):
        opname: str
        opcode: int
        arg: int | None
        argval: Any
        argrepr: str
        offset: int
        starts_line: int | None
        is_jump_target: bool
        positions: Positions | None = None

else:
    class _Instruction(NamedTuple):
        opname: str
        opcode: int
        arg: int | None
        argval: Any
        argrepr: str
        offset: int
        starts_line: int | None
        is_jump_target: bool

class Instruction(_Instruction):
    if sys.version_info < (3, 13):
        def _disassemble(self, lineno_width: int = 3, mark_as_current: bool = False, offset_width: int = 4) -> str: ...
    if sys.version_info >= (3, 13):
        @property
        def oparg(self) -> int: ...
        @property
        def baseopcode(self) -> int: ...
        @property
        def baseopname(self) -> str: ...
        @property
        def cache_offset(self) -> int: ...
        @property
        def end_offset(self) -> int: ...
        @property
        def jump_target(self) -> int: ...
        @property
        def is_jump_target(self) -> bool: ...
    if sys.version_info >= (3, 14):
        @staticmethod
        def make(
            opname: str,
            arg: int | None,
            argval: Any,
            argrepr: str,
            offset: int,
            start_offset: int,
            starts_line: bool,
            line_number: int | None,
            label: int | None = None,
            positions: Positions | None = None,
            cache_info: list[tuple[str, int, Any]] | None = None,
        ) -> Instruction: ...

class Bytecode:
    codeobj: types.CodeType
    first_line: int
    if sys.version_info >= (3, 14):
        show_positions: bool
        # 3.14 added `show_positions`
        def __init__(
            self,
            x: _HaveCodeType | str,
            *,
            first_line: int | None = None,
            current_offset: int | None = None,
            show_caches: bool = False,
            adaptive: bool = False,
            show_offsets: bool = False,
            show_positions: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 13):
        show_offsets: bool
        # 3.13 added `show_offsets`
        def __init__(
            self,
            x: _HaveCodeType | str,
            *,
            first_line: int | None = None,
            current_offset: int | None = None,
            show_caches: bool = False,
            adaptive: bool = False,
            show_offsets: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            x: _HaveCodeType | str,
            *,
            first_line: int | None = None,
            current_offset: int | None = None,
            show_caches: bool = False,
            adaptive: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self, x: _HaveCodeType | str, *, first_line: int | None = None, current_offset: int | None = None
        ) -> None: ...

    if sys.version_info >= (3, 11):
        @classmethod
        def from_traceback(cls, tb: types.TracebackType, *, show_caches: bool = False, adaptive: bool = False) -> Self: ...
    else:
        @classmethod
        def from_traceback(cls, tb: types.TracebackType) -> Self: ...

    def __iter__(self) -> Iterator[Instruction]: ...
    def info(self) -> str: ...
    def dis(self) -> str: ...

COMPILER_FLAG_NAMES: dict[int, str]

def findlabels(code: _HaveCodeType) -> list[int]: ...
def findlinestarts(code: _HaveCodeType) -> Iterator[tuple[int, int]]: ...
def pretty_flags(flags: int) -> str: ...
def code_info(x: _HaveCodeType | str) -> str: ...

if sys.version_info >= (3, 14):
    # 3.14 added `show_positions`
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None,
        *,
        file: IO[str] | None = None,
        depth: int | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
        show_positions: bool = False,
    ) -> None: ...
    def disassemble(
        co: _HaveCodeType,
        lasti: int = -1,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
        show_positions: bool = False,
    ) -> None: ...
    def distb(
        tb: types.TracebackType | None = None,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
        show_positions: bool = False,
    ) -> None: ...

elif sys.version_info >= (3, 13):
    # 3.13 added `show_offsets`
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None,
        *,
        file: IO[str] | None = None,
        depth: int | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
    ) -> None: ...
    def disassemble(
        co: _HaveCodeType,
        lasti: int = -1,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
    ) -> None: ...
    def distb(
        tb: types.TracebackType | None = None,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
    ) -> None: ...

elif sys.version_info >= (3, 11):
    # 3.11 added `show_caches` and `adaptive`
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None,
        *,
        file: IO[str] | None = None,
        depth: int | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
    ) -> None: ...
    def disassemble(
        co: _HaveCodeType, lasti: int = -1, *, file: IO[str] | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> None: ...
    def distb(
        tb: types.TracebackType | None = None, *, file: IO[str] | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> None: ...

else:
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None, *, file: IO[str] | None = None, depth: int | None = None
    ) -> None: ...
    def disassemble(co: _HaveCodeType, lasti: int = -1, *, file: IO[str] | None = None) -> None: ...
    def distb(tb: types.TracebackType | None = None, *, file: IO[str] | None = None) -> None: ...

if sys.version_info >= (3, 13):
    # 3.13 made `show_cache` `None` by default
    def get_instructions(
        x: _HaveCodeType, *, first_line: int | None = None, show_caches: bool | None = None, adaptive: bool = False
    ) -> Iterator[Instruction]: ...

elif sys.version_info >= (3, 11):
    def get_instructions(
        x: _HaveCodeType, *, first_line: int | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> Iterator[Instruction]: ...

else:
    def get_instructions(x: _HaveCodeType, *, first_line: int | None = None) -> Iterator[Instruction]: ...

def show_code(co: _HaveCodeType, *, file: IO[str] | None = None) -> None: ...

disco = disassemble
