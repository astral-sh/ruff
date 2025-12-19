"""This is an interface to Python's internal parser."""

from _typeshed import StrOrBytesPath
from collections.abc import Sequence
from types import CodeType
from typing import Any, ClassVar, final

def expr(source: str) -> STType:
    """Creates an ST object from an expression."""

def suite(source: str) -> STType:
    """Creates an ST object from a suite."""

def sequence2st(sequence: Sequence[Any]) -> STType:
    """Creates an ST object from a tree representation."""

def tuple2st(sequence: Sequence[Any]) -> STType:
    """Creates an ST object from a tree representation."""

def st2list(st: STType, line_info: bool = False, col_info: bool = False) -> list[Any]:
    """Creates a list-tree representation of an ST."""

def st2tuple(st: STType, line_info: bool = False, col_info: bool = False) -> tuple[Any, ...]:
    """Creates a tuple-tree representation of an ST."""

def compilest(st: STType, filename: StrOrBytesPath = ...) -> CodeType:
    """Compiles an ST object into a code object."""

def isexpr(st: STType) -> bool:
    """Determines if an ST object was created from an expression."""

def issuite(st: STType) -> bool:
    """Determines if an ST object was created from a suite."""

class ParserError(Exception): ...

@final
class STType:
    """Intermediate representation of a Python parse tree."""

    __hash__: ClassVar[None]  # type: ignore[assignment]
    def compile(self, filename: StrOrBytesPath = ...) -> CodeType:
        """Compile this ST object into a code object."""

    def isexpr(self) -> bool:
        """Determines if this ST object was created from an expression."""

    def issuite(self) -> bool:
        """Determines if this ST object was created from a suite."""

    def tolist(self, line_info: bool = False, col_info: bool = False) -> list[Any]:
        """Creates a list-tree representation of this ST."""

    def totuple(self, line_info: bool = False, col_info: bool = False) -> tuple[Any, ...]:
        """Creates a tuple-tree representation of this ST."""
