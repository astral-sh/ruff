"""Interface to the compiler's internal symbol tables"""

import sys
from _collections_abc import dict_keys
from collections.abc import Sequence
from typing import Any
from typing_extensions import deprecated

__all__ = ["symtable", "SymbolTable", "Class", "Function", "Symbol"]

if sys.version_info >= (3, 13):
    __all__ += ["SymbolTableType"]

def symtable(code: str, filename: str, compile_type: str) -> SymbolTable:
    """Return the toplevel *SymbolTable* for the source code.

    *filename* is the name of the file with the code
    and *compile_type* is the *compile()* mode argument.
    """

if sys.version_info >= (3, 13):
    from enum import StrEnum

    class SymbolTableType(StrEnum):
        MODULE = "module"
        FUNCTION = "function"
        CLASS = "class"
        ANNOTATION = "annotation"
        TYPE_ALIAS = "type alias"
        TYPE_PARAMETERS = "type parameters"
        TYPE_VARIABLE = "type variable"

class SymbolTable:
    def __init__(self, raw_table: Any, filename: str) -> None: ...
    if sys.version_info >= (3, 13):
        def get_type(self) -> SymbolTableType:
            """Return the type of the symbol table.

            The value returned is one of the values in
            the ``SymbolTableType`` enumeration.
            """
    else:
        def get_type(self) -> str:
            """Return the type of the symbol table.

            The values returned are 'class', 'module', 'function',
            'annotation', 'TypeVar bound', 'type alias', and 'type parameter'.
            """

    def get_id(self) -> int:
        """Return an identifier for the table."""

    def get_name(self) -> str:
        """Return the table's name.

        This corresponds to the name of the class, function
        or 'top' if the table is for a class, function or
        global respectively.
        """

    def get_lineno(self) -> int:
        """Return the number of the first line in the
        block for the table.
        """

    def is_optimized(self) -> bool:
        """Return *True* if the locals in the table
        are optimizable.
        """

    def is_nested(self) -> bool:
        """Return *True* if the block is a nested class
        or function.
        """

    def has_children(self) -> bool:
        """Return *True* if the block has nested namespaces."""

    def get_identifiers(self) -> dict_keys[str, int]:
        """Return a view object containing the names of symbols in the table."""

    def lookup(self, name: str) -> Symbol:
        """Lookup a *name* in the table.

        Returns a *Symbol* instance.
        """

    def get_symbols(self) -> list[Symbol]:
        """Return a list of *Symbol* instances for
        names in the table.
        """

    def get_children(self) -> list[SymbolTable]:
        """Return a list of the nested symbol tables."""

class Function(SymbolTable):
    def get_parameters(self) -> tuple[str, ...]:
        """Return a tuple of parameters to the function."""

    def get_locals(self) -> tuple[str, ...]:
        """Return a tuple of locals in the function."""

    def get_globals(self) -> tuple[str, ...]:
        """Return a tuple of globals in the function."""

    def get_frees(self) -> tuple[str, ...]:
        """Return a tuple of free variables in the function."""

    def get_nonlocals(self) -> tuple[str, ...]:
        """Return a tuple of nonlocals in the function."""

class Class(SymbolTable):
    if sys.version_info >= (3, 14):
        @deprecated("Deprecated since Python 3.14; will be removed in Python 3.16.")
        def get_methods(self) -> tuple[str, ...]:
            """Return a tuple of methods declared in the class."""
    else:
        def get_methods(self) -> tuple[str, ...]:
            """Return a tuple of methods declared in the class."""

class Symbol:
    def __init__(
        self, name: str, flags: int, namespaces: Sequence[SymbolTable] | None = None, *, module_scope: bool = False
    ) -> None: ...
    def is_nonlocal(self) -> bool:
        """Return *True* if the symbol is nonlocal."""

    def get_name(self) -> str:
        """Return a name of a symbol."""

    def is_referenced(self) -> bool:
        """Return *True* if the symbol is used in
        its block.
        """

    def is_parameter(self) -> bool:
        """Return *True* if the symbol is a parameter."""
    if sys.version_info >= (3, 14):
        def is_type_parameter(self) -> bool:
            """Return *True* if the symbol is a type parameter."""

    def is_global(self) -> bool:
        """Return *True* if the symbol is global."""

    def is_declared_global(self) -> bool:
        """Return *True* if the symbol is declared global
        with a global statement.
        """

    def is_local(self) -> bool:
        """Return *True* if the symbol is local."""

    def is_annotated(self) -> bool:
        """Return *True* if the symbol is annotated."""

    def is_free(self) -> bool:
        """Return *True* if a referenced symbol is
        not assigned to.
        """
    if sys.version_info >= (3, 14):
        def is_free_class(self) -> bool:
            """Return *True* if a class-scoped symbol is free from
            the perspective of a method.
            """

    def is_imported(self) -> bool:
        """Return *True* if the symbol is created from
        an import statement.
        """

    def is_assigned(self) -> bool:
        """Return *True* if a symbol is assigned to."""
    if sys.version_info >= (3, 14):
        def is_comp_iter(self) -> bool:
            """Return *True* if the symbol is a comprehension iteration variable."""

        def is_comp_cell(self) -> bool:
            """Return *True* if the symbol is a cell in an inlined comprehension."""

    def is_namespace(self) -> bool:
        """Returns *True* if name binding introduces new namespace.

        If the name is used as the target of a function or class
        statement, this will be true.

        Note that a single name can be bound to multiple objects.  If
        is_namespace() is true, the name may also be bound to other
        objects, like an int or list, that does not introduce a new
        namespace.
        """

    def get_namespaces(self) -> Sequence[SymbolTable]:
        """Return a list of namespaces bound to this name"""

    def get_namespace(self) -> SymbolTable:
        """Return the single namespace bound to this name.

        Raises ValueError if the name is bound to multiple namespaces
        or no namespace.
        """

class SymbolTableFactory:
    def new(self, table: Any, filename: str) -> SymbolTable: ...
    def __call__(self, table: Any, filename: str) -> SymbolTable: ...
