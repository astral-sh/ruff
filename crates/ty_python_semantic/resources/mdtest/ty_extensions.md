# `ty_extensions`

This document describes the internal `ty_extensions` API for creating and manipulating types as well
as testing various type system properties.

## Type extensions

The Python language itself allows us to perform a variety of operations on types. For example, we
can build a union of types like `int | None`, or we can use type constructors such as `list[int]`
and `type[int]` to create new types. But some type-level operations that we rely on in ty, like
intersections, cannot yet be expressed in Python. The `ty_extensions` module provides the
`Intersection` and `Not` type constructors (special forms) which allow us to construct these types
directly.

### Negation

```py
from typing import Literal
from ty_extensions import Not, static_assert

def negate(n1: Not[int], n2: Not[Not[int]], n3: Not[Not[Not[int]]]) -> None:
    reveal_type(n1)  # revealed: ~int
    reveal_type(n2)  # revealed: int
    reveal_type(n3)  # revealed: ~int

# error: "Special form `ty_extensions.Not` expected exactly 1 type argument, got 2"
n: Not[int, str]
# error: [invalid-type-form] "Special form `ty_extensions.Not` expected exactly 1 type argument, got 0"
o: Not[()]

p: Not[(int,)]

def static_truthiness(not_one: Not[Literal[1]]) -> None:
    # TODO: `bool` is not incorrect, but these would ideally be `Literal[True]` and `Literal[False]`
    # respectively, since all possible runtime objects that are created by the literal syntax `1`
    # are members of the type `Literal[1]`
    reveal_type(not_one is not 1)  # revealed: bool
    reveal_type(not_one is 1)  # revealed: bool

    # But these are both `bool`, rather than `Literal[True]` or `Literal[False]`
    # as there are many runtime objects that inhabit the type `~Literal[1]`
    # but still compare equal to `1`. Two examples are `1.0` and `True`.
    reveal_type(not_one != 1)  # revealed: bool
    reveal_type(not_one == 1)  # revealed: bool
```

### Intersection

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import Intersection, Not, is_subtype_of, static_assert
from typing_extensions import Literal, Never

class S: ...
class T: ...

def x(x1: Intersection[S, T], x2: Intersection[S, Not[T]]) -> None:
    reveal_type(x1)  # revealed: S & T
    reveal_type(x2)  # revealed: S & ~T

def y(y1: Intersection[int, object], y2: Intersection[int, bool], y3: Intersection[int, Never]) -> None:
    reveal_type(y1)  # revealed: int
    reveal_type(y2)  # revealed: bool
    reveal_type(y3)  # revealed: Never

def z(z1: Intersection[int, Not[Literal[1]], Not[Literal[2]]]) -> None:
    reveal_type(z1)  # revealed: int & ~Literal[1] & ~Literal[2]

class A: ...
class B: ...
class C: ...

type ABC = Intersection[A, B, C]

static_assert(is_subtype_of(ABC, A))
static_assert(is_subtype_of(ABC, B))
static_assert(is_subtype_of(ABC, C))

class D: ...

static_assert(not is_subtype_of(ABC, D))
```

### Unknown type

The `Unknown` type is a special type that we use to represent actually unknown types (no
annotation), as opposed to `Any` which represents an explicitly unknown type.

```py
from ty_extensions import Unknown, static_assert, is_assignable_to, reveal_mro

static_assert(is_assignable_to(Unknown, int))
static_assert(is_assignable_to(int, Unknown))

def explicit_unknown(x: Unknown, y: tuple[str, Unknown], z: Unknown = 1) -> None:
    reveal_type(x)  # revealed: Unknown
    reveal_type(y)  # revealed: tuple[str, Unknown]
    reveal_type(z)  # revealed: Unknown
```

`Unknown` can be subclassed, just like `Any`:

```py
class C(Unknown): ...

# revealed: (<class 'C'>, Unknown, <class 'object'>)
reveal_mro(C)

# error: "Special form `ty_extensions.Unknown` expected no type parameter"
u: Unknown[str]
```

### `AlwaysTruthy` and `AlwaysFalsy`

`AlwaysTruthy` and `AlwaysFalsy` represent the sets of all possible objects whose truthiness is
always truthy or falsy, respectively.

They do not accept any type arguments.

```py
from typing_extensions import Literal

from ty_extensions import AlwaysFalsy, AlwaysTruthy, is_subtype_of, static_assert

static_assert(is_subtype_of(Literal[True], AlwaysTruthy))
static_assert(is_subtype_of(Literal[False], AlwaysFalsy))

static_assert(not is_subtype_of(int, AlwaysFalsy))
static_assert(not is_subtype_of(str, AlwaysFalsy))

def _(t: AlwaysTruthy, f: AlwaysFalsy):
    reveal_type(t)  # revealed: AlwaysTruthy
    reveal_type(f)  # revealed: AlwaysFalsy

def f(
    a: AlwaysTruthy[int],  # error: [invalid-type-form]
    b: AlwaysFalsy[str],  # error: [invalid-type-form]
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

## Static assertions

### Basics

The `ty_extensions` module provides a `static_assert` function that can be used to enforce
properties at type-check time. The function takes an arbitrary expression and raises a type error if
the expression is not of statically known truthiness.

```py
from ty_extensions import static_assert
from typing import TYPE_CHECKING
import sys

static_assert(True)
static_assert(False)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(False or True)
static_assert(True and True)
static_assert(False or False)  # error: "Static assertion error: argument evaluates to `False`"
static_assert(False and True)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(1 + 1 == 2)
static_assert(1 + 1 == 3)  # error: "Static assertion error: argument evaluates to `False`"

static_assert("a" in "abc")
static_assert("d" in "abc")  # error: "Static assertion error: argument evaluates to `False`"

n = None
static_assert(n is None)

static_assert(TYPE_CHECKING)

static_assert(sys.version_info >= (3, 6))
```

### Narrowing constraints

Static assertions can be used to enforce narrowing constraints:

```py
from ty_extensions import static_assert

def f(x: int | None) -> None:
    if x is not None:
        static_assert(x is not None)
    else:
        static_assert(x is None)
```

### Truthy expressions

See also: <https://docs.python.org/3/library/stdtypes.html#truth-value-testing>

```py
from ty_extensions import static_assert

static_assert(True)
static_assert(False)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(None)  # error: "Static assertion error: argument of type `None` is always falsy"

static_assert(1)
static_assert(0)  # error: "Static assertion error: argument of type `Literal[0]` is always falsy"

static_assert((0,))
static_assert(())  # error: "Static assertion error: argument of type `tuple[()]` is always falsy"

static_assert("a")
static_assert("")  # error: "Static assertion error: argument of type `Literal[""]` is always falsy"

static_assert(b"a")
static_assert(b"")  # error: "Static assertion error: argument of type `Literal[b""]` is always falsy"
```

### Error messages

We provide various tailored error messages for wrong argument types to `static_assert`:

```py
from ty_extensions import static_assert

static_assert(2 * 3 == 6)

# error: "Static assertion error: argument evaluates to `False`"
static_assert(2 * 3 == 7)

# error: "Static assertion error: argument of type `bool` has an ambiguous static truthiness"
static_assert(int(2.0 * 3.0) == 6)

class InvalidBoolDunder:
    def __bool__(self) -> int:
        return 1

# error: [unsupported-bool-conversion]  "Boolean conversion is not supported for type `InvalidBoolDunder`"
static_assert(InvalidBoolDunder())
```

### Custom error messages

Alternatively, users can provide custom error messages:

```py
from ty_extensions import static_assert

# error: "Static assertion error: I really want this to be true"
static_assert(1 + 1 == 3, "I really want this to be true")

error_message = "A custom message "
error_message += "constructed from multiple string literals"
# error: "Static assertion error: A custom message constructed from multiple string literals"
static_assert(False, error_message)
```

There are limitations to what we can still infer as a string literal. In those cases, we simply fall
back to the default message:

```py
shouted_message = "A custom message".upper()
# error: "Static assertion error: argument evaluates to `False`"
static_assert(False, shouted_message)
```

## Diagnostic snapshots

```py
from ty_extensions import static_assert
import secrets

# a passing assertion
static_assert(1 < 2)
```

When the argument evalutes to `False`:

```py
# snapshot: static-assert-error
static_assert(1 > 2)
```

```snapshot
error[static-assert-error]: Static assertion error: argument evaluates to `False`
 --> src/mdtest_snippet.py:7:1
  |
7 | static_assert(1 > 2)
  | ^^^^^^^^^^^^^^-----^
  |               |
  |               Inferred type of argument is `Literal[False]`
  |
```

With a custom message:

```py
# snapshot: static-assert-error
static_assert(1 > 2, "with a message")
```

```snapshot
error[static-assert-error]: Static assertion error: with a message
 --> src/mdtest_snippet.py:9:1
  |
9 | static_assert(1 > 2, "with a message")
  | ^^^^^^^^^^^^^^-----^^^^^^^^^^^^^^^^^^^
  |               |
  |               Inferred type of argument is `Literal[False]`
  |
```

When it evaluates to something falsy:

```py
# snapshot: static-assert-error
static_assert("")
```

```snapshot
error[static-assert-error]: Static assertion error: argument of type `Literal[""]` is always falsy
  --> src/mdtest_snippet.py:11:1
   |
11 | static_assert("")
   | ^^^^^^^^^^^^^^--^
   |               |
   |               Inferred type of argument is `Literal[""]`
   |
```

When it evaluates to something that is not statically known to be truthy or falsy:

```py
# snapshot: static-assert-error
static_assert(secrets.randbelow(2))
```

```snapshot
error[static-assert-error]: Static assertion error: argument of type `int` has an ambiguous static truthiness
  --> src/mdtest_snippet.py:13:1
   |
13 | static_assert(secrets.randbelow(2))
   | ^^^^^^^^^^^^^^--------------------^
   |               |
   |               Inferred type of argument is `int`
   |
```

## Type predicates

The `ty_extensions` module also provides predicates to test various properties of types. These are
implemented as functions that return `Literal[True]` or `Literal[False]` depending on the result of
the test.

### Equivalence

```py
from ty_extensions import is_equivalent_to, static_assert
from typing_extensions import Never, Union

static_assert(is_equivalent_to(type, type[object]))
static_assert(is_equivalent_to(tuple[int, Never], Never))
static_assert(is_equivalent_to(int | str, Union[int, str]))

static_assert(not is_equivalent_to(int, str))
static_assert(not is_equivalent_to(int | str, int | str | bytes))
```

### Subtyping

```py
from ty_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(bool, int))
static_assert(not is_subtype_of(str, int))

static_assert(is_subtype_of(bool, int | str))
static_assert(is_subtype_of(str, int | str))
static_assert(not is_subtype_of(bytes, int | str))

class Base: ...
class Derived(Base): ...
class Unrelated: ...

static_assert(is_subtype_of(Derived, Base))
static_assert(not is_subtype_of(Base, Derived))
static_assert(is_subtype_of(Base, Base))

static_assert(not is_subtype_of(Unrelated, Base))
static_assert(not is_subtype_of(Base, Unrelated))
```

### Assignability

```py
from ty_extensions import is_assignable_to, static_assert
from typing import Any

static_assert(is_assignable_to(int, Any))
static_assert(is_assignable_to(Any, str))
static_assert(not is_assignable_to(int, str))
```

### Disjointness

```py
from ty_extensions import is_disjoint_from, static_assert
from typing import Literal

static_assert(is_disjoint_from(None, int))
static_assert(not is_disjoint_from(Literal[2] | str, int))
```

### Singleton types

```py
from ty_extensions import is_singleton, static_assert
from typing import Literal

static_assert(is_singleton(None))
static_assert(is_singleton(Literal[True]))

static_assert(not is_singleton(int))
static_assert(not is_singleton(Literal["a"]))
```

### Single-valued types

```py
from ty_extensions import is_single_valued, static_assert
from typing import Literal

static_assert(is_single_valued(None))
static_assert(is_single_valued(Literal[True]))
static_assert(is_single_valued(Literal["a"]))

static_assert(not is_single_valued(int))
static_assert(not is_single_valued(Literal["a"] | Literal["b"]))
```

## `TypeOf`

We use `TypeOf` to get the inferred type of an expression. This is useful when we want to refer to
it in a type expression. For example, if we want to make sure that the class literal type `str` is a
subtype of `type[str]`, we cannot use `is_subtype_of(str, type[str])`, as that would test if the
type `str` itself is a subtype of `type[str]`. Instead, we can use `TypeOf[str]` to get the type of
the expression `str`:

```py
from ty_extensions import TypeOf, is_subtype_of, static_assert

# This is incorrect and therefore fails with ...
# error: "Static assertion error: argument of type `ConstraintSet[Literal[False]]` is always falsy"
static_assert(is_subtype_of(str, type[str]))

# Correct, returns True:
static_assert(is_subtype_of(TypeOf[str], type[str]))

class Base: ...
class Derived(Base): ...
```

`TypeOf` can also be used in annotations:

```py
def type_of_annotation() -> None:
    t1: TypeOf[Base] = Base
    t2: TypeOf[(Base,)] = Derived  # error: [invalid-assignment]

    # Note how this is different from `type[…]` which includes subclasses:
    s1: type[Base] = Base
    s2: type[Base] = Derived  # no error here

# error: "Special form `ty_extensions.TypeOf` expected exactly 1 type argument, got 3"
t: TypeOf[int, str, bytes]

# error: [invalid-type-form] "`ty_extensions.TypeOf` requires exactly one argument when used in a parameter annotation"
def f(x: TypeOf) -> None:
    reveal_type(x)  # revealed: Unknown
```

## Self-referential `TypeOf` in annotations

A function can reference itself via `TypeOf` in a deferred annotation. This should not cause a stack
overflow:

```py
from ty_extensions import TypeOf

def foo(x: "TypeOf[foo]"):
    reveal_type(x)  # revealed: def foo(x: def foo(...)) -> Unknown
```

## Recursive `TypeOf` in returned callables

```toml
[environment]
python-version = "3.14"
```

```py
from __future__ import annotations

from collections.abc import Callable
from typing import Concatenate, Protocol, TypedDict
from ty_extensions import TypeOf, generic_context

def self_recursive[**P, T](
    x: Callable[Concatenate[TypeOf[self_recursive], ...], T],
) -> Callable[Concatenate[TypeOf[self_recursive], P], T]:
    return x

reveal_type(generic_context(self_recursive))  # revealed: ty_extensions.GenericContext[T@self_recursive]
# revealed: def self_recursive[T](x: (def self_recursive(...), /, *args: Any, **kwargs: Any) -> T) -> ((def self_recursive(...), /, *args: P'return.args, **kwargs: P'return.kwargs) -> T)
reveal_type(self_recursive)

def mutual_first[**P, T](
    x: Callable[Concatenate[TypeOf[mutual_second], ...], T],
) -> Callable[Concatenate[TypeOf[mutual_second], P], T]:
    return x

def mutual_second[**P, T](
    x: Callable[Concatenate[TypeOf[mutual_first], ...], T],
) -> Callable[Concatenate[TypeOf[mutual_first], P], T]:
    return x

reveal_type(generic_context(mutual_first))  # revealed: ty_extensions.GenericContext[T@mutual_first]
reveal_type(generic_context(mutual_second))  # revealed: ty_extensions.GenericContext[T@mutual_second]

class VarianceClass[T]:
    x: Callable[[TypeOf[variance_class]], T]

def variance_class[T](x: TypeOf[variance_class]) -> VarianceClass[T]:
    raise NotImplementedError

reveal_type(variance_class)  # revealed: def variance_class[T](x: def variance_class(...)) -> VarianceClass[T]

class VarianceProtocol[T](Protocol):
    x: Callable[[TypeOf[variance_protocol]], T]

def variance_protocol[T](x: TypeOf[variance_protocol]) -> VarianceProtocol[T]:
    raise NotImplementedError

reveal_type(variance_protocol)  # revealed: def variance_protocol[T](x: def variance_protocol(...)) -> VarianceProtocol[T]

class VarianceTypedDict[T](TypedDict):
    x: Callable[[TypeOf[variance_typed_dict]], T]

def variance_typed_dict[T](x: TypeOf[variance_typed_dict]) -> VarianceTypedDict[T]:
    raise NotImplementedError

reveal_type(variance_typed_dict)  # revealed: def variance_typed_dict[T](x: def variance_typed_dict(...)) -> VarianceTypedDict[T]

class Box[T]:
    @staticmethod
    def method(x: T) -> T:
        return x

def factory[T]() -> Callable[[TypeOf[Box[T].method]], T]:
    raise NotImplementedError

factory()(Box[int].method)

class Foo:
    @staticmethod
    def method[**P, T](
        x: Callable[Concatenate[TypeOf[Bar.method], ...], T],
    ) -> Callable[Concatenate[TypeOf[Bar.method], P], T]:
        return x

class Bar:
    @staticmethod
    def method[**P, T](
        x: Callable[Concatenate[TypeOf[Foo.method], ...], T],
    ) -> Callable[Concatenate[TypeOf[Foo.method], P], T]:
        return x

reveal_type(generic_context(Foo.method))  # revealed: ty_extensions.GenericContext[T@method]
reveal_type(generic_context(Bar.method))  # revealed: ty_extensions.GenericContext[T@method]

def dunder_get[**P, T](
    x: Callable[Concatenate[TypeOf[dunder_get.__get__], ...], T],
) -> Callable[Concatenate[TypeOf[dunder_get.__get__], P], T]:
    return x

reveal_type(generic_context(dunder_get))  # revealed: ty_extensions.GenericContext[T@dunder_get]

def alias_get[**P, T](
    x: Callable[Concatenate[AliasGet, ...], T],
) -> Callable[Concatenate[AliasGet, P], T]:
    return x

type AliasGet = TypeOf[alias_get.__get__]

reveal_type(generic_context(alias_get))  # revealed: ty_extensions.GenericContext[T@alias_get]

type ReturnedCallableAlias[**P] = Callable[Concatenate[TypeOf[alias_return], P], int]

def alias_return[**P](
    x: Callable[Concatenate[TypeOf[alias_return], ...], int],
) -> ReturnedCallableAlias[P]:
    return x

type ChainedReturnedCallableAlias[**P] = ReturnedCallableAliasTarget[P]
type ReturnedCallableAliasTarget[**P] = Callable[Concatenate[TypeOf[alias_chain_return], P], int]

def alias_chain_return[**P](
    x: Callable[Concatenate[TypeOf[alias_chain_return], ...], int],
) -> ChainedReturnedCallableAlias[P]:
    return x

def property_getter[**P, T](
    self: object,
) -> Callable[Concatenate[PropertyAlias, P], T]:
    raise NotImplementedError

recursive_property = property(property_getter)

type PropertyAlias = TypeOf[recursive_property]

generic_context(property_getter)
```

## Deeply nested `TypeOf` chains

Multiple redefinitions of a function with `TypeOf[foo]` as the return type create a chain of
distinct function types. The display of such chains is truncated to prevent extremely long output:

```py
from ty_extensions import TypeOf

def foo() -> TypeOf[foo]:  # error: [unresolved-reference]
    return foo

def foo() -> TypeOf[foo]:
    return foo  # error: [invalid-return-type]

def foo() -> TypeOf[foo]:
    return foo  # error: [invalid-return-type]

def foo() -> TypeOf[foo]:
    return foo  # error: [invalid-return-type]

def foo() -> TypeOf[foo]:
    return foo  # error: [invalid-return-type]

def foo() -> TypeOf[foo]:
    return foo  # error: [invalid-return-type]

# Truncated after 4 levels of function type nesting:
reveal_type(foo)  # revealed: def foo() -> def foo() -> def foo() -> def foo() -> def foo(...)
```

## `CallableTypeOf`

The `CallableTypeOf` special form can be used to extract the callable type inhabited by a given
callable object. This can be used to get the externally visible signature of the object, which can
then be used to test various type properties.

Unlike a plain `typing.Callable[...]`, `CallableTypeOf[...]` preserves function-like behavior. This
means method-like and descriptor-like callables remain distinct from regular callables in some
type-theoretic checks.

It accepts a single type parameter which is expected to be a callable object.

```py
from ty_extensions import CallableTypeOf

def f1():
    return

def f2() -> int:
    return 1

def f3(x: int, y: str) -> None:
    return

# error: [invalid-type-form] "Special form `ty_extensions.CallableTypeOf` expected exactly 1 type argument, got 2"
c1: CallableTypeOf[f1, f2]

# error: [invalid-type-form] "Expected the first argument to `ty_extensions.CallableTypeOf` to be a callable object, but got an object of type `Literal["foo"]`"
c2: CallableTypeOf["foo"]

# error: [invalid-type-form] "Expected the first argument to `ty_extensions.CallableTypeOf` to be a callable object, but got an object of type `Literal["foo"]`"
c20: CallableTypeOf[("foo",)]

# error: [invalid-type-form] "`ty_extensions.CallableTypeOf` requires exactly one argument when used in a parameter annotation"
def f(x: CallableTypeOf) -> None:
    reveal_type(x)  # revealed: Unknown

c3: CallableTypeOf[(f3,)]

# error: [invalid-type-form] "Special form `ty_extensions.CallableTypeOf` expected exactly 1 type argument, got 0"
c4: CallableTypeOf[()]
```

Using it in annotation to reveal the signature of the callable object:

```py
from typing_extensions import Self

class Foo:
    def __init__(self, x: int) -> None:
        pass

    def __call__(self, x: int) -> str:
        return "foo"

    def returns_self(self, x: int) -> Self:
        return self

    @classmethod
    def class_method(cls, x: int) -> Self:
        return cls(x)

def _(
    c1: CallableTypeOf[f1],
    c2: CallableTypeOf[f2],
    c3: CallableTypeOf[f3],
    c4: CallableTypeOf[Foo],
    c5: CallableTypeOf[Foo(42).__call__],
    c6: CallableTypeOf[Foo(42).returns_self],
    c7: CallableTypeOf[Foo.class_method],
    c8: CallableTypeOf[Foo(42)],
) -> None:
    reveal_type(c1)  # revealed: () -> Unknown
    reveal_type(c2)  # revealed: () -> int
    reveal_type(c3)  # revealed: (x: int, y: str) -> None
    reveal_type(c4)  # revealed: (x: int) -> Foo
    reveal_type(c5)  #  revealed: (x: int) -> str
    reveal_type(c6)  # revealed: (x: int) -> Foo
    reveal_type(c7)  # revealed: (x: int) -> Foo
    reveal_type(c8)  # revealed: (x: int) -> str
```

## `RegularCallableTypeOf`

The `RegularCallableTypeOf` special form also extracts a callable type from a callable object, but
it normalizes the result to a regular `typing.Callable`-style type.

This keeps the callable signatures while discarding function-like behavior. Use it when you want to
compare a callable against ordinary `Callable[...]` types without preserving descriptor semantics.

It accepts a single type parameter which is expected to be a callable object.

```py
from typing import Callable
from ty_extensions import CallableTypeOf, RegularCallableTypeOf, is_assignable_to, static_assert

def f(x: int, /) -> None: ...

static_assert(not is_assignable_to(Callable[[int], None], CallableTypeOf[f]))
static_assert(is_assignable_to(Callable[[int], None], RegularCallableTypeOf[f]))
```

## Self-referential `CallableTypeOf` and `RegularCallableTypeOf`

```toml
[environment]
python-version = "3.14"
```

```py
from ty_extensions import CallableTypeOf, RegularCallableTypeOf

def callable[T]() -> CallableTypeOf[callable]:
    raise NotImplementedError

def regular[T]() -> RegularCallableTypeOf[regular]:
    raise NotImplementedError

def call() -> CallableTypeOf[call.__call__]:
    raise NotImplementedError

def first() -> CallableTypeOf[second]:
    raise NotImplementedError

def second() -> CallableTypeOf[first]:
    raise NotImplementedError

def regular_first() -> RegularCallableTypeOf[regular_second]:
    raise NotImplementedError

def regular_second() -> RegularCallableTypeOf[regular_first]:
    raise NotImplementedError

reveal_type(callable)  # revealed: def callable[T]() -> ((*args: object, **kwargs: object) -> Never)
reveal_type(regular)  # revealed: def regular[T]() -> ((*args: object, **kwargs: object) -> Never)
reveal_type(call)  # revealed: def call() -> ((*args: object, **kwargs: object) -> Never)
reveal_type(first)  # revealed: def first() -> (() -> Divergent)
reveal_type(second)  # revealed: def second() -> (() -> (() -> Divergent))
reveal_type(regular_first)  # revealed: def regular_first() -> (() -> Divergent)
reveal_type(regular_second)  # revealed: def regular_second() -> (() -> (() -> Divergent))
```
