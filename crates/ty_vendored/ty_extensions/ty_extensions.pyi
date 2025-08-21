import sys
from collections.abc import Iterable
from enum import Enum
from typing import (
    Any,
    ClassVar,
    LiteralString,
    Protocol,
    _SpecialForm,
)

from typing_extensions import Self  # noqa: UP035

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
def is_singleton(type: Any) -> bool: ...
def is_single_valued(type: Any) -> bool: ...

# Returns the generic context of a type as a tuple of typevars, or `None` if the
# type is not generic.
def generic_context(type: Any) -> Any: ...

# Returns the `__all__` names of a module as a tuple of sorted strings, or `None` if
# either the module does not have `__all__` or it has invalid elements.
def dunder_all_names(module: Any) -> Any: ...

# List all members of an enum.
def enum_members[E: type[Enum]](enum: E) -> tuple[str, ...]: ...

# Returns the type that's an upper bound of materializing the given (gradual) type.
def top_materialization(type: Any) -> Any: ...

# Returns the type that's a lower bound of materializing the given (gradual) type.
def bottom_materialization(type: Any) -> Any: ...

# Returns a tuple of all members of the given object, similar to `dir(obj)` and
# `inspect.getmembers(obj)`, with at least the following differences:
#
# * `dir` and `inspect.getmembers` may use runtime mutable state to construct
# the list of attributes returned. In contrast, this routine is limited to
# static information only.
# * `dir` will respect an object's `__dir__` implementation, if present, but
# this method (currently) does not.
def all_members(obj: Any) -> tuple[str, ...]: ...

# Returns `True` if the given object has a member with the given name.
def has_member(obj: Any, name: str) -> bool: ...

# Passing a protocol type to this function will cause ty to emit an info-level
# diagnostic describing the protocol's interface. Passing a non-protocol type
# will cause ty to emit an error diagnostic.
def reveal_protocol_interface(protocol: type) -> None: ...

# A protocol describing an interface that should be satisfied by all named tuples
# created using `typing.NamedTuple` or `collections.namedtuple`.
class NamedTupleLike(Protocol):
    # from typing.NamedTuple stub
    _field_defaults: ClassVar[dict[str, Any]]
    _fields: ClassVar[tuple[str, ...]]
    @classmethod
    def _make(self: Self, iterable: Iterable[Any]) -> Self: ...
    def _asdict(self, /) -> dict[str, Any]: ...
    def _replace(self: Self, /, **kwargs) -> Self: ...
    if sys.version_info >= (3, 13):
        def __replace__(self: Self, **kwargs) -> Self: ...
