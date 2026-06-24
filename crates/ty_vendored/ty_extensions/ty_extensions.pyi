# ruff: noqa: PYI021
import collections.abc
import sys
import types
from enum import Enum
from typing import Any, ClassVar, Protocol, _SpecialForm

from typing_extensions import LiteralString, Self, TypeForm  # noqa: UP035

# Special operations
def static_assert(condition: object, msg: LiteralString | None = None) -> None: ...

# -----
# Types
# -----

Unknown: _SpecialForm
"""
`Unknown` is a dynamic type inferred due to missing type information or an inference error.

ty infers `Unknown` for unannotated values with insufficient type information. It also uses it as a
fallback after certain type errors. This contrasts with `Any`, which represents an *explicitly*
annotated dynamic type. Like `Any`, however, it is a dynamic type, so ty allows any operation on it.
"""

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

AlwaysTruthy: _SpecialForm
"""
`AlwaysTruthy` represents the set of all objects that always evaluate to `True` in a boolean
context.

`AlwaysTruthy` is inhabited by singleton objects such as `True` and `...`, as well as truthy
literal strings, integers and bytestrings. It can also be inhabited by instances of classes
with `__bool__` methods returning `Literal[True]`.

In practice, most Python objects inhabit neither `AlwaysTruthy` nor `AlwaysFalsy`, since
their boolean evaluation may be uncertain or depend on runtime state. For example, although
an instance of *exactly* `object` is always truthy, a variable annotated as having type
`object` could also be an instance of an arbitrary subclass of `object` that is always falsy.
This means that the boolean evaluation of a variable inferred as `object` is uncertain, so
`object` is not a subtype of `AlwaysTruthy`.
"""

AlwaysFalsy: _SpecialForm
"""
`AlwaysFalsy` represents the set of all objects that always evaluate to `False` in a boolean
context.

`AlwaysFalsy` is inhabited by singleton objects such as `False` and `None`, as well as the
literals `""`, `b""` and `0`. It can also be inhabited by instances of classes with `__bool__`
methods returning `Literal[False]`.

In practice, however, most Python objects inhabit neither `AlwaysTruthy` nor `AlwaysFalsy`,
since their boolean evaluation may be uncertain or depend on runtime state.
"""

# -------------
# Special forms
# -------------

Not: _SpecialForm
"""`Not[T]` represents the set of all objects that do not inhabit the type `T`."""

Intersection: _SpecialForm
"""
`Intersection[T1, T2, ..., Tn]` represents an intersection type: the set of all objects that inhabit
all of the types `T1`, `T2`, ..., `Tn`.

For any two fully static types `T1` and `T2`, `Intersection[T1, T2]` is a subtype of both `T1` and
`T2`. For any type `T3` that is a subtype of both `T1` and `T2`, `Intersection[T1, T2]` is a
supertype of `T3`.

In the following example, although neither `P` nor `Q` is a subtype of the other, an instance of `S`
inhabits `Intersection[P, Q]` because `S` inherits from both `P` and `Q`:

```python
class P: ...
class Q: ...
class S(P, Q): ...

s: Intersection[P, Q] = S()
```
"""

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

Top: _SpecialForm
"""
`Top[T]` represents the "top materialization" of `T`.

For any type `T`, the top [materialization] of `T` is a type that is
a supertype of all materializations of `T`.

For a [fully static] type `T`, `Top[T]` is always exactly the same type
as `T` itself. For example, the top materialization of `Sequence[int]`
is simply `Sequence[int]`.

For a [gradual type] `T` that contains [`Any`][Any] or `Unknown` inside
it, however, `Top[T]` will not be equivalent to `T`. `Top[Sequence[Any]]`
evaluates to `Sequence[object]`: since `Sequence` is covariant, no
possible materialization of `Any` exists such that a fully static
materialization of `Sequence[Any]` would not be a subtype of
`Sequence[object]`.

`Top[T]` cannot be simplified further for invariant gradual types.
`Top[list[Any]]` cannot be simplified to any other type: because `list`
is invariant, `list[object]` is not a supertype of `list[int]`. The
top materialization of `list[Any]` is simply `Top[list[Any]]`: the
infinite union of `list[T]` for every possible fully static type `T`.

[materialization]: https://typing.python.org/en/latest/spec/concepts.html#materialization
[fully static]: https://typing.python.org/en/latest/spec/concepts.html#fully-static-types
[gradual type]: https://typing.python.org/en/latest/spec/concepts.html#gradual-types
[Any]: https://typing.python.org/en/latest/spec/special-types.html#any
"""

Bottom: _SpecialForm
"""
`Bottom[T]` represents the "bottom materialization" of `T`.

For any type `T`, the bottom [materialization] of `T` is a type that is
a subtype of all materializations of `T`.

For a [fully static] type `T`, `Bottom[T]` is always exactly the same type
as `T` itself. For example, the bottom materialization of `Sequence[int]`
is simply `Sequence[int]`.

For a [gradual type] `T` that contains [`Any`][Any] or `Unknown` inside it,
however, `Bottom[T]` will not be equivalent to `T`. `Bottom[Sequence[Any]]`
evaluates to `Sequence[Never]`: since `Sequence` is covariant, no
possible materialization of `Any` exists such that a fully static
materialization of `Sequence[Any]` would not be a supertype of
`Sequence[Never]`. (`Sequence[Never]` is not the same type as the
uninhabited type `Never`: for example, it is inhabited by the empty tuple,
`()`.)

For many invariant gradual types `T`, `Bottom[T]` is equivalent to
[`Never`][Never], although ty will not necessarily apply this simplification
eagerly.

[materialization]: https://typing.python.org/en/latest/spec/concepts.html#materialization
[fully static]: https://typing.python.org/en/latest/spec/concepts.html#fully-static-types
[gradual type]: https://typing.python.org/en/latest/spec/concepts.html#gradual-types
[Never]: https://typing.python.org/en/latest/spec/special-types.html#never
[Any]: https://typing.python.org/en/latest/spec/special-types.html#any
"""

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

    def satisfied_by_all_typevars(
        self, *, inferable: TypeForm[tuple[object, ...]] | None = None
    ) -> bool:
        """
        Returns whether this constraint set is satisfied by all of the typevars
        that it mentions. You must provide a tuple of the typevars that should
        be considered `inferable`. All other typevars mentioned in the
        constraint set will be considered non-inferable.
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

# Predicates on types
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

def is_single_valued(ty: TypeForm[object]) -> bool:
    """Returns `True` if `ty` is non-empty and all inhabitants compare equal to each other."""

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

class NamedTupleLike(Protocol):
    """
    A protocol describing an interface that should be satisfied by all named tuples
    created using `typing.NamedTuple` or `collections.namedtuple`.
    """

    # _fields is defined as `tuple[Any, ...]` rather than `tuple[str, ...]` so
    # that instances of actual `NamedTuple` classes with more precise `_fields`
    # types are considered assignable to this protocol (protocol attribute members
    # are invariant, and `tuple[str, str]` is not invariantly assignable to
    # `tuple[str, ...]`).
    _fields: ClassVar[tuple[Any, ...]]
    _field_defaults: ClassVar[dict[str, Any]]
    @classmethod
    def _make(cls: type[Self], iterable: collections.abc.Iterable[Any]) -> Self: ...
    def _asdict(self, /) -> dict[str, Any]: ...

    # Positional arguments aren't actually accepted by these methods at runtime,
    # but adding the `*args` parameters means that all `NamedTuple` classes
    # are understood as assignable to this protocol due to the special case
    # outlined in https://typing.python.org/en/latest/spec/callables.html#meaning-of-in-callable:
    #
    # > If the input signature in a function definition includes both a
    # > `*args` and `**kwargs` parameter and both are typed as `Any`
    # > (explicitly or implicitly because it has no annotation), a type
    # > checker should treat this as the equivalent of `...`.
    def _replace(self, *args, **kwargs) -> Self: ...
    if sys.version_info >= (3, 13):
        def __replace__(self, *args, **kwargs) -> Self: ...

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
