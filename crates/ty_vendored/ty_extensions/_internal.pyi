# ruff: noqa: PYI021
import types
from enum import Enum
from typing import Any, Protocol, _SpecialForm

from typing_extensions import LiteralString, Self, TypeForm  # noqa: UP035

# -------------
# Special forms
# -------------

TypeOf: _SpecialForm
"""
`TypeOf[expression]` is the inferred type of `expression`.

Unlike a regular [type expression], the argument to `TypeOf` is interpreted as a
["value expression"][value expression]: an ordinary Python expression whose type ty infers. Whereas
`str` in a type annotation means "any instance of the class `str`", `TypeOf[str]` in a type annotation
signifies "the type inhabited by the `str` class object itself at runtime". `Literal[3]` is therefore
the same type as `TypeOf[3]`, since ty infers the object `3` as having type `Literal[3]`.

[type expression]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
[value expression]: https://docs.python.org/3/reference/expressions.html
"""

CallableTypeOf: _SpecialForm
"""
`CallableTypeOf[T]` extracts the callable type of `T` while preserving any function-like
behavior.

This means the result may behave differently from a plain `typing.Callable` in
type-theoretic checks. In particular, method-like and descriptor-like callables remain
distinct from regular callables.

Use this when you want to preserve the full callable flavor of a function, method, or
synthesized callable.
"""

RegularCallableTypeOf: _SpecialForm
"""
`RegularCallableTypeOf[T]` extracts a regular `typing.Callable`-style type from `T`.

This keeps the callable signatures of `T` but discards function-like behavior such as
descriptor-style method binding. Use this when you want to compare a callable against
ordinary `Callable[...]` types in type-theoretic tests.
"""

# -----
# Types
# -----

Todo: _SpecialForm
"""
`@Todo` is a dynamic type inferred due to a known missing feature or incomplete implementation in
ty.

Like `Any` and `Unknown`, `@Todo` is a dynamic type, so ty allows any operation on it. Unlike `Any`,
it is not explicitly provided in an annotation; unlike `Unknown`, it specifically indicates a
limitation in ty.

It is an internal type used by ty and cannot be used in annotations. These types should disappear
as ty implements the missing functionality.
"""

Divergent: _SpecialForm
"""
`Divergent` is a dynamic type inferred due to type-level recursion that does not converge.

Type inference can be recursive. ty analyzes inference cycles repeatedly, looking for a
stable result. If each iteration produces a new type, ty replaces the non-convergent part
with `Divergent`.

Like `Any` and `Unknown`, `Divergent` is a dynamic type, so ty allows any operation on it.
Unlike `Unknown`, it does not represent missing type information. It is an internal type
used by ty and cannot be used in annotations.
"""

# -----------
# Constraints
# -----------

class ConstraintSetSolution:
    """One solution path for a constraint set."""

class ConstraintSet:
    @staticmethod
    def range(
        lower_bound: TypeForm[object],
        typevar: TypeForm[object],
        upper_bound: TypeForm[object],
    ) -> ConstraintSet:
        """
        Returns a constraint set that requires `typevar` to specialize to a type
        that is a supertype of `lower_bound` and a subtype of `upper_bound`.
        """

    @staticmethod
    def always() -> ConstraintSet:
        """Returns a constraint set that is always satisfied"""

    @staticmethod
    def never() -> ConstraintSet:
        """Returns a constraint set that is never satisfied"""

    def implies_subtype_of(self, ty: TypeForm[object], of: TypeForm[object]) -> Self:
        """
        Returns a constraint set that is satisfied when `ty` is a `subtype`_ of
        `of`, assuming that all of the constraints in `self` hold.

        .. _subtype: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
        """

    def satisfies(self, other: Self) -> Self:
        """
        Returns whether this constraint set satisfies another — that is, whether
        every specialization that satisfies this constraint set also satisfies
        `other`.
        """

    def exists(self, typevars: TypeForm[tuple[object, ...]]) -> Self:
        """
        Existentially abstracts the given type variables from this constraint set.
        """

    def for_all(self, typevars: TypeForm[tuple[object, ...]]) -> Self:
        """
        Universally abstracts the given type variables from this constraint set.
        """

    def satisfied_by_all_typevars(
        self, *, inferable: TypeForm[tuple[object, ...]] | None = None
    ) -> bool:
        """
        Returns whether this constraint set is satisfied by all of the typevars
        that it mentions. You must provide a tuple of the typevars that should
        be considered `inferable`. All other typevars mentioned in the
        constraint set will be considered non-inferable.
        """

    def solutions_for(
        self,
        typevar: TypeForm[object],
        *,
        inferable: TypeForm[tuple[object, ...]],
    ) -> tuple[ConstraintSetSolution, ...] | None:
        """
        Returns the explicit solutions inferred for `typevar` across all paths.

        `inferable` specifies all typevars that should be solved for. Every
        solution path is preserved, with its bindings filtered to `typevar`.
        Returns `None` if the constraint set is unsatisfiable.
        """

    def solutions(
        self, *, inferable: TypeForm[tuple[object, ...]]
    ) -> tuple[ConstraintSetSolution, ...] | None:
        """
        Returns all explicit solutions, preserving path and binding order.

        `inferable` specifies all typevars that should be solved for. Each
        solution contains the bindings inferred on one satisfying path.
        Returns `None` if the constraint set is unsatisfiable.
        """

    def __bool__(self) -> bool: ...
    def __eq__(self, other: ConstraintSet) -> bool: ...
    def __ne__(self, other: ConstraintSet) -> bool: ...
    def __and__(self, other: ConstraintSet) -> ConstraintSet: ...
    def __or__(self, other: ConstraintSet) -> ConstraintSet: ...
    def __invert__(self) -> ConstraintSet: ...
    def with_detailed_display(self) -> ConstraintSet:
        """
        Returns a copy of this constraint set that will display the full
        constraint formula when rendered as a string.

        Typically we only display "bool" for a non-trivial constraint set, to
        help ensure that we do not write test cases that depend on how
        constraint sets are rendered. But it can be useful to see the full
        detail for debugging purposes.
        """

class GenericContext:
    """
    The set of typevars that are bound by a generic class, function, or type
    alias.
    """

class Specialization:
    """A mapping of typevars to specific types"""

# -------------------
# Predicates on types
# -------------------

def is_equivalent_to(
    type_a: TypeForm[object], type_b: TypeForm[object]
) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `type_a` and `type_b` are
    `equivalent`_ types.

    .. _equivalent: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
    """

def is_subtype_of(ty: TypeForm[object], of: TypeForm[object]) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is a `subtype`_ of `of`.

    .. _subtype: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    """

def is_assignable_to(ty: TypeForm[object], to: TypeForm[object]) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is `assignable`_ to `to`.

    .. _assignable: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    """

def is_constraint_set_assignable_to(
    ty: TypeForm[object],
    to: TypeForm[object],
) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `ty` is `assignable`_ to `to`.

    This differs from `is_assignable_to` in how it treats typevars.
    `is_assignable_to` will assume that all typevars are non-inferable, and will
    require all possible specializations of a typevar to satisfy the relation.
    This method will instead return a constraint set describing which
    specializations (possibly not all of them) satisfy the relation.

    .. _assignable: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    """

def is_disjoint_from(
    type_a: TypeForm[object], type_b: TypeForm[object]
) -> ConstraintSet:
    """Returns a constraint set that is satisfied when `type_a` and `type_b` are disjoint types.

    Two types are disjoint if they have no inhabitants in common.
    """

def is_singleton(ty: TypeForm[object]) -> bool:
    """Returns `True` if `ty` is a singleton type with exactly one inhabitant."""

# -------------------
# Operations on types
# -------------------

def generic_context(input: Any) -> GenericContext | None:
    """Returns the generic context of the input (a class, a function, a method, a type alias, ..)
    as a tuple of typevars.

    Returns `None` if the input is not generic.
    """

def into_callable(value: Any) -> Any:
    """Converts a value into a `Callable`, if possible.

    This is the value equivalent of `CallableTypeOf`, which operates on types.
    """

def into_regular_callable(value: Any) -> Any:
    """Converts a value into a regular `Callable`, if possible.

    This is the value equivalent of `RegularCallableTypeOf`, which operates on types.
    """

def dunder_all_names(module: Any) -> tuple[LiteralString, ...] | None:
    """Returns the `__all__` names of a module as a tuple of sorted strings.

    Returns `None` if either the module does not have `__all__` or it has invalid elements.
    """

def enum_members[E: type[Enum]](enum: E) -> tuple[LiteralString, ...]:
    """List all members of an enum."""

def all_members(obj: Any) -> tuple[LiteralString, ...]:
    """Returns a tuple of all members of the given object.

    This nearly emulates `dir(obj)` and `inspect.getmembers(obj)` in Python's
    standard library, but has at least the following differences:

    * `dir` and `inspect.getmembers` may use runtime mutable state to construct
      the list of attributes returned. In contrast, this routine is limited to
      static information only.
    * `dir` will respect an object's `__dir__` implementation, if present, but
      this method (currently) does not.
    """

def has_member(obj: Any, name: LiteralString) -> bool:
    """Returns `True` if the given object has a member with the given name."""

def reveal_protocol_interface(protocol: type) -> None:
    """
    Passing a protocol type to this function will cause ty to emit an info-level
    diagnostic describing the protocol's interface.

    Passing a non-protocol type will cause ty to emit an error diagnostic.
    """

def reveal_mro(cls: type | types.GenericAlias) -> None:
    """Reveal the MRO that ty infers for the given class or generic alias."""

# Special variants of the corresponding protocols in `typing`, but without the
# `__iter__`/`__aiter__` on Iterator/AsyncIterator, which is often omitted in
# practice. These protocols are used for generating better `non-iterable` error
# messages, nothing else.
class Iterator[T](Protocol):
    def __next__(self, /) -> T: ...

class Iterable[T](Protocol):
    def __iter__(self, /) -> Iterator[T]: ...

class AsyncIterator[T](Protocol):
    async def __anext__(self, /) -> T: ...

class AsyncIterable[T](Protocol):
    def __aiter__(self, /) -> AsyncIterator[T]: ...
