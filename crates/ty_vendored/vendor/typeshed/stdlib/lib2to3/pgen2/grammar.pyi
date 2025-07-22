"""This module defines the data structures used to represent a grammar.

These are a bit arcane because they are derived from the data
structures used by Python's 'pgen' parser generator.

There's also a table here mapping operators to their names in the
token module; the Python tokenize module reports all operators as the
fallback token code OP, but the parser needs the actual token code.

"""

from _typeshed import StrPath
from typing_extensions import Self, TypeAlias

_Label: TypeAlias = tuple[int, str | None]
_DFA: TypeAlias = list[list[tuple[int, int]]]
_DFAS: TypeAlias = tuple[_DFA, dict[int, int]]

class Grammar:
    """Pgen parsing tables conversion class.

    Once initialized, this class supplies the grammar tables for the
    parsing engine implemented by parse.py.  The parsing engine
    accesses the instance variables directly.  The class here does not
    provide initialization of the tables; several subclasses exist to
    do this (see the conv and pgen modules).

    The load() method reads the tables from a pickle file, which is
    much faster than the other ways offered by subclasses.  The pickle
    file is written by calling dump() (after loading the grammar
    tables using a subclass).  The report() method prints a readable
    representation of the tables to stdout, for debugging.

    The instance variables are as follows:

    symbol2number -- a dict mapping symbol names to numbers.  Symbol
                     numbers are always 256 or higher, to distinguish
                     them from token numbers, which are between 0 and
                     255 (inclusive).

    number2symbol -- a dict mapping numbers to symbol names;
                     these two are each other's inverse.

    states        -- a list of DFAs, where each DFA is a list of
                     states, each state is a list of arcs, and each
                     arc is a (i, j) pair where i is a label and j is
                     a state number.  The DFA number is the index into
                     this list.  (This name is slightly confusing.)
                     Final states are represented by a special arc of
                     the form (0, j) where j is its own state number.

    dfas          -- a dict mapping symbol numbers to (DFA, first)
                     pairs, where DFA is an item from the states list
                     above, and first is a set of tokens that can
                     begin this grammar rule (represented by a dict
                     whose values are always 1).

    labels        -- a list of (x, y) pairs where x is either a token
                     number or a symbol number, and y is either None
                     or a string; the strings are keywords.  The label
                     number is the index in this list; label numbers
                     are used to mark state transitions (arcs) in the
                     DFAs.

    start         -- the number of the grammar's start symbol.

    keywords      -- a dict mapping keyword strings to arc labels.

    tokens        -- a dict mapping token numbers to arc labels.
    """

    symbol2number: dict[str, int]
    number2symbol: dict[int, str]
    states: list[_DFA]
    dfas: dict[int, _DFAS]
    labels: list[_Label]
    keywords: dict[str, int]
    tokens: dict[int, int]
    symbol2label: dict[str, int]
    start: int
    def dump(self, filename: StrPath) -> None:
        """Dump the grammar tables to a pickle file."""

    def load(self, filename: StrPath) -> None:
        """Load the grammar tables from a pickle file."""

    def copy(self) -> Self:
        """Copy the grammar."""

    def report(self) -> None:
        """Dump the grammar tables to standard output, for debugging."""

opmap_raw: str
opmap: dict[str, str]
