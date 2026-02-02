"""Disassembler of Python byte code into mnemonics."""

import sys
import types
from collections.abc import Callable, Iterator
from opcode import *  # `dis` re-exports it as a part of public API
from typing import IO, Any, Final, NamedTuple
from typing_extensions import Self, TypeAlias, disjoint_base

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
        """Positions(lineno, end_lineno, col_offset, end_col_offset)"""

        lineno: int | None = None
        end_lineno: int | None = None
        col_offset: int | None = None
        end_col_offset: int | None = None

if sys.version_info >= (3, 13):
    class _Instruction(NamedTuple):
        """_Instruction(opname, opcode, arg, argval, argrepr, offset, start_offset, starts_line, line_number, label, positions, cache_info)"""

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
        """_Instruction(opname, opcode, arg, argval, argrepr, offset, starts_line, is_jump_target, positions)"""

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
        """_Instruction(opname, opcode, arg, argval, argrepr, offset, starts_line, is_jump_target)"""

        opname: str
        opcode: int
        arg: int | None
        argval: Any
        argrepr: str
        offset: int
        starts_line: int | None
        is_jump_target: bool

if sys.version_info >= (3, 12):
    class Instruction(_Instruction):
        """Details for a bytecode operation.

        Defined fields:
          opname - human readable name for operation
          opcode - numeric code for operation
          arg - numeric argument to operation (if any), otherwise None
          argval - resolved arg value (if known), otherwise same as arg
          argrepr - human readable description of operation argument
          offset - start index of operation within bytecode sequence
          start_offset - start index of operation within bytecode sequence including extended args if present;
                         otherwise equal to Instruction.offset
          starts_line - True if this opcode starts a source line, otherwise False
          line_number - source line number associated with this opcode (if any), otherwise None
          label - A label if this instruction is a jump target, otherwise None
          positions - Optional dis.Positions object holding the span of source code
                      covered by this instruction
          cache_info - information about the format and content of the instruction's cache
                         entries (if any)
        """

        if sys.version_info < (3, 13):
            def _disassemble(self, lineno_width: int = 3, mark_as_current: bool = False, offset_width: int = 4) -> str:
                """Format instruction details for inclusion in disassembly output

                *lineno_width* sets the width of the line number field (0 omits it)
                *mark_as_current* inserts a '-->' marker arrow as part of the line
                *offset_width* sets the width of the instruction offset field
                """
        if sys.version_info >= (3, 13):
            @property
            def oparg(self) -> int:
                """Alias for Instruction.arg."""

            @property
            def baseopcode(self) -> int:
                """Numeric code for the base operation if operation is specialized.

                Otherwise equal to Instruction.opcode.
                """

            @property
            def baseopname(self) -> str:
                """Human readable name for the base operation if operation is specialized.

                Otherwise equal to Instruction.opname.
                """

            @property
            def cache_offset(self) -> int:
                """Start index of the cache entries following the operation."""

            @property
            def end_offset(self) -> int:
                """End index of the cache entries following the operation."""

            @property
            def jump_target(self) -> int:
                """Bytecode index of the jump target if this is a jump operation.

                Otherwise return None.
                """

            @property
            def is_jump_target(self) -> bool:
                """True if other code jumps to here, otherwise False"""
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

else:
    @disjoint_base
    class Instruction(_Instruction):
        """Details for a bytecode operation

        Defined fields:
          opname - human readable name for operation
          opcode - numeric code for operation
          arg - numeric argument to operation (if any), otherwise None
          argval - resolved arg value (if known), otherwise same as arg
          argrepr - human readable description of operation argument
          offset - start index of operation within bytecode sequence
          starts_line - line started by this opcode (if any), otherwise None
          is_jump_target - True if other code jumps to here, otherwise False
          positions - Optional dis.Positions object holding the span of source code
                      covered by this instruction
        """

        def _disassemble(self, lineno_width: int = 3, mark_as_current: bool = False, offset_width: int = 4) -> str:
            """Format instruction details for inclusion in disassembly output

            *lineno_width* sets the width of the line number field (0 omits it)
            *mark_as_current* inserts a '-->' marker arrow as part of the line
            *offset_width* sets the width of the instruction offset field
            """

class Bytecode:
    """The bytecode operations of a piece of code

    Instantiate this with a function, method, other compiled object, string of
    code, or a code object (as returned by compile()).

    Iterating over this yields the bytecode operations as Instruction instances.
    """

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
        def from_traceback(cls, tb: types.TracebackType, *, show_caches: bool = False, adaptive: bool = False) -> Self:
            """Construct a Bytecode from the given traceback"""
    else:
        @classmethod
        def from_traceback(cls, tb: types.TracebackType) -> Self:
            """Construct a Bytecode from the given traceback"""

    def __iter__(self) -> Iterator[Instruction]: ...
    def info(self) -> str:
        """Return formatted information about the code object."""

    def dis(self) -> str:
        """Return a formatted view of the bytecode operations."""

COMPILER_FLAG_NAMES: Final[dict[int, str]]

def findlabels(code: _HaveCodeType) -> list[int]:
    """Detect all offsets in a byte code which are jump targets.

    Return the list of offsets.

    """

def findlinestarts(code: _HaveCodeType) -> Iterator[tuple[int, int]]:
    """Find the offsets in a byte code which are start of lines in the source.

    Generate pairs (offset, lineno)
    lineno will be an integer or None the offset does not have a source line.
    """

def pretty_flags(flags: int) -> str:
    """Return pretty representation of code flags."""

def code_info(x: _HaveCodeType | str) -> str:
    """Formatted details of methods, functions, or code."""

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
    ) -> None:
        """Disassemble classes, methods, functions, and other compiled objects.

        With no argument, disassemble the last traceback.

        Compiled objects currently include generator objects, async generator
        objects, and coroutine objects, all of which store their code object
        in a special attribute.
        """

    def disassemble(
        co: _HaveCodeType,
        lasti: int = -1,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
        show_positions: bool = False,
    ) -> None:
        """Disassemble a code object."""

    def distb(
        tb: types.TracebackType | None = None,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
        show_positions: bool = False,
    ) -> None:
        """Disassemble a traceback (default: last traceback)."""

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
    ) -> None:
        """Disassemble classes, methods, functions, and other compiled objects.

        With no argument, disassemble the last traceback.

        Compiled objects currently include generator objects, async generator
        objects, and coroutine objects, all of which store their code object
        in a special attribute.
        """

    def disassemble(
        co: _HaveCodeType,
        lasti: int = -1,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
    ) -> None:
        """Disassemble a code object."""

    def distb(
        tb: types.TracebackType | None = None,
        *,
        file: IO[str] | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
        show_offsets: bool = False,
    ) -> None:
        """Disassemble a traceback (default: last traceback)."""

elif sys.version_info >= (3, 11):
    # 3.11 added `show_caches` and `adaptive`
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None,
        *,
        file: IO[str] | None = None,
        depth: int | None = None,
        show_caches: bool = False,
        adaptive: bool = False,
    ) -> None:
        """Disassemble classes, methods, functions, and other compiled objects.

        With no argument, disassemble the last traceback.

        Compiled objects currently include generator objects, async generator
        objects, and coroutine objects, all of which store their code object
        in a special attribute.
        """

    def disassemble(
        co: _HaveCodeType, lasti: int = -1, *, file: IO[str] | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> None:
        """Disassemble a code object."""

    def distb(
        tb: types.TracebackType | None = None, *, file: IO[str] | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> None:
        """Disassemble a traceback (default: last traceback)."""

else:
    def dis(
        x: _HaveCodeType | str | bytes | bytearray | None = None, *, file: IO[str] | None = None, depth: int | None = None
    ) -> None:
        """Disassemble classes, methods, functions, and other compiled objects.

        With no argument, disassemble the last traceback.

        Compiled objects currently include generator objects, async generator
        objects, and coroutine objects, all of which store their code object
        in a special attribute.
        """

    def disassemble(co: _HaveCodeType, lasti: int = -1, *, file: IO[str] | None = None) -> None:
        """Disassemble a code object."""

    def distb(tb: types.TracebackType | None = None, *, file: IO[str] | None = None) -> None:
        """Disassemble a traceback (default: last traceback)."""

if sys.version_info >= (3, 13):
    # 3.13 made `show_cache` `None` by default
    def get_instructions(
        x: _HaveCodeType, *, first_line: int | None = None, show_caches: bool | None = None, adaptive: bool = False
    ) -> Iterator[Instruction]:
        """Iterator for the opcodes in methods, functions or code

        Generates a series of Instruction named tuples giving the details of
        each operations in the supplied code.

        If *first_line* is not None, it indicates the line number that should
        be reported for the first source line in the disassembled code.
        Otherwise, the source line information (if any) is taken directly from
        the disassembled code object.
        """

elif sys.version_info >= (3, 11):
    def get_instructions(
        x: _HaveCodeType, *, first_line: int | None = None, show_caches: bool = False, adaptive: bool = False
    ) -> Iterator[Instruction]:
        """Iterator for the opcodes in methods, functions or code

        Generates a series of Instruction named tuples giving the details of
        each operations in the supplied code.

        If *first_line* is not None, it indicates the line number that should
        be reported for the first source line in the disassembled code.
        Otherwise, the source line information (if any) is taken directly from
        the disassembled code object.
        """

else:
    def get_instructions(x: _HaveCodeType, *, first_line: int | None = None) -> Iterator[Instruction]:
        """Iterator for the opcodes in methods, functions or code

        Generates a series of Instruction named tuples giving the details of
        each operations in the supplied code.

        If *first_line* is not None, it indicates the line number that should
        be reported for the first source line in the disassembled code.
        Otherwise, the source line information (if any) is taken directly from
        the disassembled code object.
        """

def show_code(co: _HaveCodeType, *, file: IO[str] | None = None) -> None:
    """Print details of methods, functions, or code to *file*.

    If *file* is not provided, the output is printed on stdout.
    """

disco = disassemble
