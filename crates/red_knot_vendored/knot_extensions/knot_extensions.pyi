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
