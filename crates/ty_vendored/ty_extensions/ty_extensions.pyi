from typing import Any, LiteralString, _SpecialForm

# Special operations
def static_assert(condition: object, msg: LiteralString | None = None) -> None: ...

# Types
Unknown = object()
AlwaysTruthy = object()
AlwaysFalsy = object()

# Special forms
Not: _SpecialForm
Intersection: _SpecialForm
TypeOf: _SpecialForm
CallableTypeOf: _SpecialForm

# ty treats annotations of `float` to mean `float | int`, and annotations of `complex`
# to mean `complex | float | int`. This is to support a typing-system special case [1].
# We therefore provide `JustFloat` and `JustComplex` to represent the "bare" `float` and
# `complex` types, respectively.
#
# [1]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
type JustFloat = TypeOf[1.0]
type JustComplex = TypeOf[1.0j]

# Predicates on types
#
# Ideally, these would be annotated using `TypeForm`, but that has not been
# standardized yet (https://peps.python.org/pep-0747).
def is_equivalent_to(type_a: Any, type_b: Any) -> bool: ...
def is_subtype_of(type_derived: Any, type_base: Any) -> bool: ...
def is_assignable_to(type_target: Any, type_source: Any) -> bool: ...
def is_disjoint_from(type_a: Any, type_b: Any) -> bool: ...
def is_gradual_equivalent_to(type_a: Any, type_b: Any) -> bool: ...
def is_fully_static(type: Any) -> bool: ...
def is_singleton(type: Any) -> bool: ...
def is_single_valued(type: Any) -> bool: ...

# Returns the generic context of a type as a tuple of typevars, or `None` if the
# type is not generic.
def generic_context(type: Any) -> Any: ...

# Returns the `__all__` names of a module as a tuple of sorted strings, or `None` if
# either the module does not have `__all__` or it has invalid elements.
def dunder_all_names(module: Any) -> Any: ...
