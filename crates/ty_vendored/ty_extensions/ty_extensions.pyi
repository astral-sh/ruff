# ruff: noqa: PYI021
import sys
import types
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
# Top[T] evaluates to the top materialization of T, a type that is a supertype
# of every materialization of T.
Top: _SpecialForm
# Bottom[T] evaluates to the bottom materialization of T, a type that is a subtype
# of every materialization of T.
Bottom: _SpecialForm

# ty treats annotations of `float` to mean `float | int`, and annotations of `complex`
# to mean `complex | float | int`. This is to support a typing-system special case [1].
# We therefore provide `JustFloat` and `JustComplex` to represent the "bare" `float` and
# `complex` types, respectively.
#
# [1]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
type JustFloat = TypeOf[1.0]
type JustComplex = TypeOf[1.0j]

# Constraints
class ConstraintSet:
    def __bool__(self) -> bool: ...
    def __eq__(self, other: ConstraintSet) -> bool: ...
    def __ne__(self, other: ConstraintSet) -> bool: ...
    def __and__(self, other: ConstraintSet) -> ConstraintSet: ...
    def __or__(self, other: ConstraintSet) -> ConstraintSet: ...
    def __invert__(self) -> ConstraintSet: ...

def range_constraint(
    lower_bound: Any, typevar: Any, upper_bound: Any
) -> ConstraintSet: ...
def negated_range_constraint(
    lower_bound: Any, typevar: Any, upper_bound: Any
) -> ConstraintSet: ...

# Predicates on types
#
# Ideally, these would be annotated using `TypeForm`, but that has not been
# standardized yet (https://peps.python.org/pep-0747).
def is_equivalent_to(type_a: Any, type_b: Any) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `type_a` and `type_b` are
    `equivalent`_ types.

    .. _equivalent: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
    """

def is_subtype_of(ty: Any, of: Any) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is a `subtype`_ of `of`.

    .. _subtype: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    """

def is_subtype_of_given(
    constraints: bool | ConstraintSet, ty: Any, of: Any
) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is a `subtype`_ of `of`,
    assuming that all of the constraints in `constraints` hold.

    .. _subtype: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    """

def is_assignable_to(ty: Any, to: Any) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is `assignable`_ to `to`.

    .. _assignable: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    """

def is_disjoint_from(type_a: Any, type_b: Any) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `type_a` and `type_b` are disjoint types.

    Two types are disjoint if they have no inhabitants in common.
    """

def is_singleton(ty: Any) -> bool:
    """Returns `True` if `ty` is a singleton type with exactly one inhabitant."""

def is_single_valued(ty: Any) -> bool:
    """Returns `True` if `ty` is non-empty and all inhabitants compare equal to each other."""

# Returns the generic context of a type as a tuple of typevars, or `None` if the
# type is not generic.
def generic_context(ty: Any) -> Any: ...

# Returns the `__all__` names of a module as a tuple of sorted strings, or `None` if
# either the module does not have `__all__` or it has invalid elements.
def dunder_all_names(module: Any) -> Any: ...

# List all members of an enum.
def enum_members[E: type[Enum]](enum: E) -> tuple[str, ...]: ...

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
def reveal_protocol_interface(protocol: type) -> None:
    """
    Passing a protocol type to this function will cause ty to emit an info-level
    diagnostic describing the protocol's interface.

    Passing a non-protocol type will cause ty to emit an error diagnostic.
    """

def reveal_mro(cls: type | types.GenericAlias) -> None:
    """Reveal the MRO that ty infers for the given class or generic alias."""

# A protocol describing an interface that should be satisfied by all named tuples
# created using `typing.NamedTuple` or `collections.namedtuple`.
class NamedTupleLike(Protocol):
    # from typing.NamedTuple stub
    _field_defaults: ClassVar[dict[str, Any]]
    _fields: ClassVar[tuple[str, ...]]
    @classmethod
    def _make(cls: type[Self], iterable: Iterable[Any]) -> Self: ...
    def _asdict(self, /) -> dict[str, Any]: ...
    def _replace(self, /, **kwargs) -> Self: ...
    if sys.version_info >= (3, 13):
        def __replace__(self, **kwargs) -> Self: ...
