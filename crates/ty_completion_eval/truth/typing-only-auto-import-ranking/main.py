from typing import TYPE_CHECKING

# Runtime symbols outrank alternatives from typing-only modules in Python files.
deprecated<CURSOR: warnings.deprecated>
NoneTy<CURSOR: types.NoneType>
Not<CURSOR: ast.Not>

# Typing-only symbols are included in auto-import suggestions.
static_ass<CURSOR: ty_extensions.static_assert>
is_equiv<CURSOR: ty_extensions._internal.is_equivalent_to>
TypedDictFall<CURSOR: _typeshed._type_checker_internals.TypedDictFallback>

# Typing-only symbols retain their usual ranking inside TYPE_CHECKING blocks.
if TYPE_CHECKING:
    deprecated<CURSOR: typing_extensions.deprecated>
    NoneTy<CURSOR: _typeshed.NoneType>


def function_scope() -> None:
    if TYPE_CHECKING:
        deprecated<CURSOR: typing_extensions.deprecated>
