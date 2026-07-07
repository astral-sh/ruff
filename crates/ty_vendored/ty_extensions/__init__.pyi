# ruff: noqa: PYI021
import collections.abc
import sys
from typing import Any, ClassVar, Protocol, _SpecialForm

from typing_extensions import LiteralString, Self  # noqa: UP035

from .internal import TypeOf as _TypeOf

# ------------------
# Special operations
# ------------------

def static_assert(condition: object, msg: LiteralString | None = None) -> None: ...

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

# ty treats annotations of `float` to mean `float | int`, and annotations of `complex`
# to mean `complex | float | int`. This is to support a typing-system special case [1].
# We therefore provide `JustFloat` and `JustComplex` to represent the "bare" `float` and
# `complex` types, respectively.
#
# [1]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
type JustFloat = _TypeOf[1.0]
type JustComplex = _TypeOf[1.0j]

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
