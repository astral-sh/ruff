"""Support to pretty-print lists, tuples, & dictionaries recursively.

Very simple, but useful, especially in debugging data structures.

Classes
-------

PrettyPrinter()
    Handle pretty-printing operations onto a stream using a configured
    set of formatting parameters.

Functions
---------

pformat()
    Format a Python object into a pretty-printed representation.

pprint()
    Pretty-print a Python object to a stream [default is sys.stdout].

saferepr()
    Generate a 'standard' repr()-like value, but protect against recursive
    data structures.

"""

import sys
from _typeshed import SupportsWrite
from collections import deque
from typing import IO

__all__ = ["pprint", "pformat", "isreadable", "isrecursive", "saferepr", "PrettyPrinter", "pp"]

if sys.version_info >= (3, 10):
    def pformat(
        object: object,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = True,
        underscore_numbers: bool = False,
    ) -> str:
        """Format a Python object into a pretty-printed representation."""

else:
    def pformat(
        object: object,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = True,
    ) -> str:
        """Format a Python object into a pretty-printed representation."""

if sys.version_info >= (3, 10):
    def pp(
        object: object,
        stream: IO[str] | None = None,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = False,
        underscore_numbers: bool = False,
    ) -> None:
        """Pretty-print a Python object"""

else:
    def pp(
        object: object,
        stream: IO[str] | None = None,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = False,
    ) -> None:
        """Pretty-print a Python object"""

if sys.version_info >= (3, 10):
    def pprint(
        object: object,
        stream: IO[str] | None = None,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = True,
        underscore_numbers: bool = False,
    ) -> None:
        """Pretty-print a Python object to a stream [default is sys.stdout]."""

else:
    def pprint(
        object: object,
        stream: IO[str] | None = None,
        indent: int = 1,
        width: int = 80,
        depth: int | None = None,
        *,
        compact: bool = False,
        sort_dicts: bool = True,
    ) -> None:
        """Pretty-print a Python object to a stream [default is sys.stdout]."""

def isreadable(object: object) -> bool:
    """Determine if saferepr(object) is readable by eval()."""

def isrecursive(object: object) -> bool:
    """Determine if object requires a recursive representation."""

def saferepr(object: object) -> str:
    """Version of repr() which can handle recursive data structures."""

class PrettyPrinter:
    if sys.version_info >= (3, 10):
        def __init__(
            self,
            indent: int = 1,
            width: int = 80,
            depth: int | None = None,
            stream: IO[str] | None = None,
            *,
            compact: bool = False,
            sort_dicts: bool = True,
            underscore_numbers: bool = False,
        ) -> None:
            """Handle pretty printing operations onto a stream using a set of
            configured parameters.

            indent
                Number of spaces to indent for each level of nesting.

            width
                Attempted maximum number of columns in the output.

            depth
                The maximum depth to print out nested structures.

            stream
                The desired output stream.  If omitted (or false), the standard
                output stream available at construction will be used.

            compact
                If true, several items will be combined in one line.

            sort_dicts
                If true, dict keys are sorted.

            underscore_numbers
                If true, digit groups are separated with underscores.

            """
    else:
        def __init__(
            self,
            indent: int = 1,
            width: int = 80,
            depth: int | None = None,
            stream: IO[str] | None = None,
            *,
            compact: bool = False,
            sort_dicts: bool = True,
        ) -> None:
            """Handle pretty printing operations onto a stream using a set of
            configured parameters.

            indent
                Number of spaces to indent for each level of nesting.

            width
                Attempted maximum number of columns in the output.

            depth
                The maximum depth to print out nested structures.

            stream
                The desired output stream.  If omitted (or false), the standard
                output stream available at construction will be used.

            compact
                If true, several items will be combined in one line.

            sort_dicts
                If true, dict keys are sorted.

            """

    def pformat(self, object: object) -> str: ...
    def pprint(self, object: object) -> None: ...
    def isreadable(self, object: object) -> bool: ...
    def isrecursive(self, object: object) -> bool: ...
    def format(self, object: object, context: dict[int, int], maxlevels: int, level: int) -> tuple[str, bool, bool]:
        """Format object for a specific context, returning a string
        and flags indicating whether the representation is 'readable'
        and whether the object represents a recursive construct.
        """

    def _format(
        self, object: object, stream: SupportsWrite[str], indent: int, allowance: int, context: dict[int, int], level: int
    ) -> None: ...
    def _pprint_dict(
        self,
        object: dict[object, object],
        stream: SupportsWrite[str],
        indent: int,
        allowance: int,
        context: dict[int, int],
        level: int,
    ) -> None: ...
    def _pprint_list(
        self, object: list[object], stream: SupportsWrite[str], indent: int, allowance: int, context: dict[int, int], level: int
    ) -> None: ...
    def _pprint_tuple(
        self,
        object: tuple[object, ...],
        stream: SupportsWrite[str],
        indent: int,
        allowance: int,
        context: dict[int, int],
        level: int,
    ) -> None: ...
    def _pprint_set(
        self, object: set[object], stream: SupportsWrite[str], indent: int, allowance: int, context: dict[int, int], level: int
    ) -> None: ...
    def _pprint_deque(
        self, object: deque[object], stream: SupportsWrite[str], indent: int, allowance: int, context: dict[int, int], level: int
    ) -> None: ...
    def _format_dict_items(
        self,
        items: list[tuple[object, object]],
        stream: SupportsWrite[str],
        indent: int,
        allowance: int,
        context: dict[int, int],
        level: int,
    ) -> None: ...
    def _format_items(
        self, items: list[object], stream: SupportsWrite[str], indent: int, allowance: int, context: dict[int, int], level: int
    ) -> None: ...
    def _repr(self, object: object, context: dict[int, int], level: int) -> str: ...
    if sys.version_info >= (3, 10):
        def _safe_repr(self, object: object, context: dict[int, int], maxlevels: int, level: int) -> tuple[str, bool, bool]: ...
