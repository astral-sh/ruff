# ruff: file-ignore[docstring-in-stub]
"""ty-extensions is a module that provides experimental typing features supported by ty.

ty bundles the typing stubs for this module, which means that you can use them without
installing anything. However, if you rely on runtime evaluation of type annotations,
you should add the [`ty-extensions` package](https://pypi.org/project/ty-extensions/)
as a dependency in your project:

```bash
uv add ty-extensions
```
"""

import collections.abc
import sys
from typing import Any, ClassVar, Protocol, _SpecialForm

from typing_extensions import LiteralString, Self  # ruff: ignore[deprecated-import]

from ._internal import TypeOf as _TypeOf

# ------------------
# Special operations
# ------------------

def static_assert(condition: object, msg: LiteralString | None = None) -> None:
    """Make sure that a certain condition holds at type-check time.

    Example:

    ```python
    static_assert(sys.version_info[:2] >= (3, 13), "Python 3.13 or higher is required")
    ```

    At runtime, a false condition raises `AssertionError` (even with Python optimization enabled)."""

# -------------
# Special forms
# -------------

Not: _SpecialForm
"""
`Not[T]`, also spelled as `~T`, represents the set of all objects that do not inhabit the type `T`.

In type expressions, ty supports the experimental spelling `~T`, and also uses it when displaying
negation types. While the experimental syntax will raise an exception in runtime contexts or in
cases where an annotation expression could be evaluated, however, `Not[]` can be used in arbitrary
contexts if the `ty-extensions` package is installed.
"""

Intersection: _SpecialForm
"""
`Intersection[T1, T2, ..., Tn]`, also spelled as `T1 & T2 & ... & Tn`, represents an intersection
type: the set of all objects that inhabit all of the types `T1`, `T2`, ..., `Tn`.

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

In type expressions, ty supports the experimental spelling `T1 & T2`, and also uses this syntax
when displaying intersection types. While the experimental syntax will raise an exception in
runtime contexts or in cases where an annotation expression could be evaluated, however,
`Intersection[]` can be used in arbitrary contexts if the `ty-extensions` package is installed.

Nested intersections are flattened at runtime, and duplicate operands are removed using
runtime equality, keeping the first occurrence of each. A named alias created using the
`type` statement is always kept as an operand. Intersections are unordered types, so
nongeneric intersections always compare equal at runtime when they have the same
operands:

```pycon
>>> from ty_extensions import Intersection
>>> Intersection[Intersection[str, int], str]
ty_extensions.Intersection[str, int]
>>> type Alias = Intersection[str, int]  # Python 3.12+
>>> Intersection[Alias, str]
ty_extensions.Intersection[Alias, str]
>>> Intersection[int, str] == Intersection[str, int]
True
>>> hash(Intersection[int, str]) == hash(Intersection[str, int])
True

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

AlwaysTruthy: _SpecialForm
"""
`AlwaysTruthy` represents the set of all objects that always evaluate to `True` in a boolean
context.

`AlwaysTruthy` is inhabited by singleton objects such as `True` and `...`, as well as truthy
literal strings, integers and bytestrings. It can also be inhabited by instances of classes
with `__bool__` methods returning `Literal[True]`.

In practice, most Python objects inhabit neither `AlwaysTruthy` nor `AlwaysFalsy`, since their
boolean evaluation may be uncertain or depend on runtime state. For example, although an
instance of *exactly* `object` is always truthy, a variable annotated as having type `object`
could also be an instance of an arbitrary subclass of `object` that is always falsy. This means
that the boolean evaluation of a variable inferred as `object` is uncertain, so `object` is
not a subtype of `AlwaysTruthy`.
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

type JustFloat = _TypeOf[1.0]
"""
`JustFloat` represents an instance of the class `float`. This can be used to circumvent
the [type-system special case] that lets `float` annotations accept values of
type `int` as well.

At runtime, `JustFloat` is an alias for the built-in `float` class.

[type-system special case]: https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
"""

type JustComplex = _TypeOf[1.0j]
"""
`JustComplex` represents an instance of the class `complex`. This can be used to circumvent
the [type-system special case] that lets `complex` annotations accept values of
type `float` or `int` as well.

At runtime, `JustComplex` is an alias for the built-in `complex` class.

[type-system special case]: https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
"""

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
