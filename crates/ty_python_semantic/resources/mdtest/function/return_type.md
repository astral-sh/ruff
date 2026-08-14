# Function return type

When a function's return type is annotated, all return statements are checked to ensure that the
type of the returned value is assignable to the annotated return type.

## Basic examples

A return value assignable to the annotated return type is valid.

```py
def f() -> int:
    return 1
```

The type of the value obtained by calling a function is the annotated return type, not the inferred
return type.

```py
reveal_type(f())  # revealed: int
```

A `raise` is equivalent to a return of `Never`, which is assignable to any annotated return type.

```py
def f() -> str:
    raise ValueError()

reveal_type(f())  # revealed: str
```

## Stub functions

"Stub" function definitions (that is, function definitions with an empty body) are permissible in
stub files, or in a few other locations: Protocol method definitions, abstract methods, and
overloads. In this case the function body is considered to be omitted (thus no return type checking
is performed on it), not assumed to implicitly return `None`.

A stub function's "empty" body may contain only an optional docstring, followed (optionally) by an
ellipsis (`...`) or `pass`.

### In stub file

```pyi
def f() -> int: ...
def f() -> int:
    pass

def f() -> int:
    """Some docstring"""

def f() -> int:
    """Some docstring"""
    ...
```

### In Protocol

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, TypeVar

class Bar(Protocol):
    def f(self) -> int: ...

class Baz(Bar):
    # error: [empty-body]
    def f(self) -> int: ...

T = TypeVar("T")

class Qux(Protocol[T]):
    def f(self) -> int: ...

class Foo(Protocol):
    def f[T](self, v: T) -> T: ...

t = (Protocol, int)
reveal_type(t[0])  # revealed: <special-form 'typing.Protocol'>

class Lorem(t[0]):
    def f(self) -> int: ...
```

### In abstract method

```toml
[environment]
python-version = "3.12"
```

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @abstractmethod
    def f(self) -> int: ...
    @abstractmethod
    def g[T](self, x: T) -> T: ...

class Bar[T](ABC):
    @abstractmethod
    def f(self) -> int: ...
    @abstractmethod
    def g[U](self, x: U) -> U: ...

# error: [empty-body]
def f() -> int: ...
@abstractmethod  # Semantically meaningless, accepted nevertheless
def g() -> int: ...
```

### In overload

```py
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str):
    return x
```

### In `if TYPE_CHECKING` block

Inside an `if TYPE_CHECKING` block, we allow "stub" style function definitions with empty bodies,
since these functions will never actually be called.

`compat/__init__.py`:

```py
```

`compat/sub/__init__.py`:

```py
```

`compat/sub/sub.py`:

```py
from typing import TYPE_CHECKING
```

`main.py`:

```py
from typing import TYPE_CHECKING
import typing
import typing as t
import compat.sub.sub

if TYPE_CHECKING:
    def f() -> int: ...

else:
    def f() -> str:
        return "hello"

reveal_type(f)  # revealed: def f() -> int

if not TYPE_CHECKING:
    pass
elif True:
    def g() -> str: ...

else:
    def h() -> str: ...

if not TYPE_CHECKING:
    def i() -> int:
        return 1

else:
    def i() -> str: ...

reveal_type(i)  # revealed: def i() -> str

if False:
    pass
elif TYPE_CHECKING:
    def j() -> str: ...

else:
    def j():
        raise NotImplementedError

if False:
    pass
elif not TYPE_CHECKING:
    def k() -> str:
        raise NotImplementedError

else:
    def k() -> str: ...

class Foo:
    if TYPE_CHECKING:
        def f(self) -> int: ...

if TYPE_CHECKING:
    class Bar:
        def f(self) -> int: ...

def get_bool() -> bool:
    return True

if TYPE_CHECKING:
    if get_bool():
        def l() -> str: ...

if get_bool():
    if TYPE_CHECKING:
        def m() -> str: ...

if TYPE_CHECKING:
    if not TYPE_CHECKING:
        def n() -> str: ...

if typing.TYPE_CHECKING:
    def o() -> str: ...

if not typing.TYPE_CHECKING:
    def p() -> str:
        raise NotImplementedError

if compat.sub.sub.TYPE_CHECKING:
    def q() -> str: ...

if not compat.sub.sub.TYPE_CHECKING:
    def r() -> str:
        raise NotImplementedError

if t.TYPE_CHECKING:
    def s() -> str: ...

if not t.TYPE_CHECKING:
    def t() -> str:
        raise NotImplementedError
```

## Conditional return type

```py
def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        return 2

def f(cond: bool) -> int | None:
    if cond:
        return 1
    else:
        return

def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        raise ValueError()

def f(cond: bool) -> str | int:
    if cond:
        return "a"
    else:
        return 1
```

## Implicit return type

```py
def f(cond: bool) -> int | None:
    if cond:
        return 1

# no implicit return
def f() -> int:
    if True:
        return 1

# no implicit return
def f(cond: bool) -> int:
    cond = True
    if cond:
        return 1

def f(cond: bool) -> int:
    if cond:
        cond = True
    else:
        return 1
    if cond:
        return 2
```

## Invalid return type

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

```py
# error: [invalid-return-type]
def f() -> int:
    1

def f() -> str:
    # error: [invalid-return-type]
    return 1

def f() -> int:
    # error: [invalid-return-type]
    return

from typing import TypeVar

T = TypeVar("T")

# error: [empty-body]
def m(x: T) -> T: ...

class A[T]: ...

def f() -> A[int]:
    class A[T]: ...
    return A[int]()  # error: [invalid-return-type]

class B: ...

def g() -> B:
    class B: ...
    return B()  # error: [invalid-return-type]
```

## Invalid return type in stub file

<!-- snapshot-diagnostics -->

```pyi
def f() -> int:
    # error: [invalid-return-type]
    return ...

# error: [invalid-return-type]
def foo() -> int:
    print("...")
    ...

# error: [invalid-return-type]
def foo() -> int:
    f"""{foo} is a function that ..."""
    ...
```

## Invalid conditional return type

<!-- snapshot-diagnostics -->

```py
def f(cond: bool) -> str:
    if cond:
        return "a"
    else:
        # error: [invalid-return-type]
        return 1

def f(cond: bool) -> str:
    if cond:
        # error: [invalid-return-type]
        return 1
    else:
        # error: [invalid-return-type]
        return 2
```

## Invalid implicit return type

<!-- snapshot-diagnostics -->

```py
def f() -> None:
    if False:
        return 1

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        return 1

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        raise ValueError()

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        cond = False
    else:
        return 1
    if cond:
        return 2
```

## Invalid implicit return type always None

<!-- snapshot-diagnostics -->

If the function has no `return` statement or if it has only bare `return` statement (no variable in
the return statement), then we show a diagnostic hint that the return annotation should be `-> None`
or a `return` statement should be added.

```py
# error: [invalid-return-type]
def f() -> int:
    print("hello")
```

## NotImplemented

### Default Python version

`NotImplemented` is a special symbol in Python. It is commonly used to control the fallback behavior
of special dunder methods. You can find more details in the
[documentation](https://docs.python.org/3/library/numbers.html#implementing-the-arithmetic-operations).

```py
from __future__ import annotations

class A:
    def __add__(self, o: A) -> A:
        return NotImplemented
```

However, as shown below, `NotImplemented` should not cause issues with the declared return type.

```py
def f() -> int:
    return NotImplemented

def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        return NotImplemented

def f(x: int) -> int | str:
    if x < 0:
        return -1
    elif x == 0:
        return NotImplemented
    else:
        return "test"

def f(cond: bool) -> str:
    return "hello" if cond else NotImplemented

def f(cond: bool) -> int:
    # error: [invalid-return-type] "Return type does not match returned value: expected `int`, found `Literal["hello"]`"
    return "hello" if cond else NotImplemented
```

`NotImplemented` is only special-cased for return types (mirroring the way the interpreter applies
special casing for the symbol at runtime). It is not generally considered assignable to every other
type:

```py
# Other type checkers do not emit an error here,
# but this is likely not a deliberate feature they've implemented;
# it's probably because `NotImplementedType` inherits from `Any`
# according to typeshed. We override typeshed's incorrect MRO
# for more precise type inference.
x: int = NotImplemented  # error: [invalid-assignment]
```

### Python 3.10+

We correctly understand the semantics of `NotImplemented` on all Python versions, even though the
class `types.NotImplementedType` is only exposed in the `types` module on Python 3.10+.

```toml
[environment]
python-version = "3.10"
```

```py
def f() -> int:
    return NotImplemented

def f(cond: bool) -> str:
    return "hello" if cond else NotImplemented
```

## Generator functions

<!-- snapshot-diagnostics -->

### Synchronous

A function with a `yield` or `yield from` expression anywhere in its body is a
[generator function](https://docs.python.org/3/glossary.html#term-generator). A generator function
implicitly returns an instance of `types.GeneratorType` even if it does not contain any `return`
statements.

```py
import types
import typing

def f() -> types.GeneratorType[int, None, None]:
    yield 42

def g() -> typing.Generator[int]:
    yield 42

def h() -> typing.Iterator[int]:
    yield 42

def i() -> typing.Iterable[int]:
    yield 42

def i2() -> typing.Generator[int]:
    yield from i()

def j() -> str:  # error: [invalid-return-type]
    yield 42

def invalid_return_type() -> typing.Generator[None, None, None]:
    yield
    return ""  # error: [invalid-return-type]
```

The return value of the function must be assignable to the return type of the `Generator`. This is
specified in the third type parameter.

```py
def wrong_return() -> typing.Generator[int, int, int]:
    yield 1
    return ""  # error: [invalid-return-type]
```

If the function has no return and it's implicitly returning it is still type checked.

```py
def bare_return_ok() -> typing.Generator[int, int, None]:
    yield 1

def missing_return() -> typing.Generator[int, int, int]:  # error: [invalid-return-type]
    yield 1
```

Iterators must not return anything.

```py
def iterator_must_not_return() -> typing.Iterator[int]:
    yield 2
    # error: [invalid-return-type]
    return "foo"
```

### Asynchronous

If it is an `async` function with a `yield` statement in its body, it is an
[asynchronous generator function](https://docs.python.org/3/glossary.html#term-asynchronous-generator).
An asynchronous generator function implicitly returns an instance of `types.AsyncGeneratorType` even
if it does not contain any `return` statements.

```py
import types
import typing

async def f() -> types.AsyncGeneratorType[int, None]:
    yield 42

async def g() -> typing.AsyncGenerator[int]:
    yield 42

async def h() -> typing.AsyncIterator[int]:
    yield 42

async def i() -> typing.AsyncIterable[int]:
    yield 42

async def j() -> str:  # error: [invalid-return-type]
    yield 42

async def k() -> typing.AsyncGenerator[int]:
    yield 42
    return 2  # error: [invalid-syntax] "`return` with value in async generator"
```

## Diagnostics for `empty-body` on non-protocol subclasses of protocol classes

<!-- snapshot-diagnostics -->

We emit a nice subdiagnostic in this situation explaining the probable error here:

```py
from typing_extensions import Protocol

class Abstract(Protocol):
    def method(self) -> str: ...

class Concrete(Abstract):
    def method(self) -> str: ...  # error: [empty-body]
```

## Diagnostics for `invalid-return-type` on dynamic type

```toml
environment.python-version = "3.12"
```

```py
from typing import Never, Any

def f(func: Any) -> Never:  # error: [invalid-return-type]
    func()
```

## `unsound-return-statement`

In addition to `invalid-return-type`, we also offer a disabled-by-default stricter rule
`unsound-return-statement`. This rule forbids `return` statements that return an instance of a type
`A` unless `A` is a *subtype* of the annotated return type:

```toml
[rules]
unsound-return-statement = "error"
```

```py
from typing import Any

# no error, even though `str` is not a subtype of `Any`:
# the lint only applies to a function if its return annotation is not a dynamic
# type such as `Any`
def returns_any() -> Any:
    return "foo"

def g() -> int:
    # snapshot: unsound-return-statement
    return returns_any()
```

```snapshot
error[unsound-return-statement]: Unsound return statement
  --> src/mdtest_snippet.py:11:12
   |
 9 | def g() -> int:
   |            --- Expected a subtype of `int` because of the return type
10 |     # snapshot: unsound-return-statement
11 |     return returns_any()
   |            ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type prior to the `return` statement
```

An example with nested error context:

```py
def h() -> tuple[tuple[int, int]]:
    # snapshot: unsound-return-statement
    return ((42, returns_any()),)
```

```snapshot
error[unsound-return-statement]: Unsound return statement
  --> src/mdtest_snippet.py:14:12
   |
12 | def h() -> tuple[tuple[int, int]]:
   |            ---------------------- Expected a subtype of `tuple[tuple[int, int]]` because of the return type
13 |     # snapshot: unsound-return-statement
14 |     return ((42, returns_any()),)
   |            ^^^^^^^^^^^^^^^^^^^^^^ Inferred as `tuple[tuple[Literal[42], Any]]`
info: `tuple[tuple[Literal[42], Any]]` is assignable to `tuple[tuple[int, int]]`, but not a subtype of `tuple[tuple[int, int]]`
info: the first tuple element is not compatible: `tuple[Literal[42], Any]` is not a subtype of `tuple[int, int]`
info: └── the second tuple element is not compatible: `Any` is not a subtype of `int`
help: Consider using an `assert` to narrow the type prior to the `return` statement
```

The rule is also applied to generator functions:

```py
from typing import Generator

def f() -> Generator[None, None, int]:
    yield
    # snapshot: unsound-return-statement
    return returns_any()
```

```snapshot
error[unsound-return-statement]: Unsound return statement
  --> src/mdtest_snippet.py:20:12
   |
17 | def f() -> Generator[None, None, int]:
   |            -------------------------- Expected a subtype of `int` because of the return type
18 |     yield
19 |     # snapshot: unsound-return-statement
20 |     return returns_any()
   |            ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type prior to the `return` statement
```

Aliases of `Any` are also dynamic return annotations and must not trigger the rule:

```py
from typing_extensions import TypeAliasType

AnyAlias = TypeAliasType("AnyAlias", Any)

def returns_any_alias() -> AnyAlias:
    return "foo"
```

The same applies when an alias of `Any` is the return type of a generator:

```py
def generator_returns_any_alias() -> Generator[None, None, AnyAlias]:
    yield
    return "foo"
```

The rule in fact will not trigger if `Any` appears anywhere in your return type, either implicitly
or explicitly:

```py
from typing import Any

# error: [missing-type-argument]
def returns_unparameterized_tuple() -> tuple:
    # no error, since the return type is implicitly `tuple[Any, ...]`
    # (which is what the `missing-type-argument` error is complaining about on the line above!)
    return returns_any()

def returns_tuple_of_any() -> tuple[Any, Any]:
    # no error, since the return type is explicitly `tuple[Any, Any]`
    return returns_any()
```

Edge case: for `TypeIs`-annotated functions, we want the error message to say "not a subtype of
`bool`" rather than "not a subtype of `TypeIs`":

```py
from typing_extensions import TypeIs

def f(x: object) -> TypeIs[int]:
    # snapshot: unsound-return-statement
    return returns_any()
```

```snapshot
error[unsound-return-statement]: Unsound return statement
  --> src/mdtest_snippet.py:45:12
   |
43 | def f(x: object) -> TypeIs[int]:
   |                     ----------- Expected a subtype of `bool` because of the return type
44 |     # snapshot: unsound-return-statement
45 |     return returns_any()
   |            ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `bool`, but not a subtype of `bool`
help: Consider using an `assert` to narrow the type prior to the `return` statement
```

Aliases of `TypeIs` still return `bool`, so diagnostics must mention `bool` rather than the alias:

```py
TypeIsAlias = TypeAliasType("TypeIsAlias", TypeIs[int])

def returns_type_is_alias(value: object) -> TypeIsAlias:
    # error: "Unsound return statement: `Any` is not a subtype of `bool`"
    return returns_any()
```

Detailed error context for aliases of `TypeIs` must also compare each union member against `bool`,
rather than against the original `TypeIs` annotation:

```py
def returns_type_is_alias_union(value: object, result: bool | Any) -> TypeIsAlias:
    # snapshot: unsound-return-statement
    return result
```

```snapshot
error[unsound-return-statement]: Unsound return statement
  --> src/mdtest_snippet.py:53:12
   |
51 | def returns_type_is_alias_union(value: object, result: bool | Any) -> TypeIsAlias:
   |                                                                       ----------- Expected a subtype of `bool` because of the return type
52 |     # snapshot: unsound-return-statement
53 |     return result
   |            ^^^^^^ Inferred as `bool | Any`
info: `bool | Any` is assignable to `bool`, but not a subtype of `bool`
info: element `Any` of union `bool | Any` is not a subtype of `bool`
help: Consider using an `assert` to narrow the type prior to the `return` statement
```

A `Never` return annotation is still a typed boundary, so returning `Any` must trigger the rule:

```py
from typing_extensions import Never

def never_returns() -> Never:
    return returns_any()  # error: [unsound-return-statement]
```

The same applies when `Never` is the return type of a generator:

```py
def generator_never_returns() -> Generator[None, None, Never]:
    yield
    return returns_any()  # error: [unsound-return-statement]
```

There is currently a limitation in how this rule interacts with contextual inference for collection
literals. When a function is annotated as returning `list[int]`, the annotation is used as context
while inferring the type of a list literal in a `return` statement. As a result, a list literal
containing an `Any` value is inferred as `list[int]` rather than `list[Any]`. The rule therefore
does not emit a diagnostic for the following unsound return statement. Mypy's `--warn-return-any`
option has the same limitation. In fact, mypy only rejects return expressions whose entire type is
`Any`, whereas this rule also rejects an independently inferred `list[Any]` when the annotated
return type is `list[int]`:

```py
def returns_list_containing_any() -> list[int]:
    return [returns_any()]
```

## Regression test: `unsound-return-statement` with gradual generic declarations

A specialized generic type is fully static if it has been specialized with fully static types, even
if the type parameter(s) it is generic over have non-fully-static bounds, constraints, or defaults.
A previous version of the rule incorrectly considered these specialized generic types as being
non-fully-static, leading to false negatives in the below examples:

```toml
[environment]
python-version = "3.13"

[rules]
unsound-return-statement = "error"
```

```py
from typing import Any, Generator, Generic, TypeVar

class Bounded[T: Any]: ...
class Constrained[T: (int, Any)]: ...
class Defaulted[T = Any]: ...

# `Bounded[int]`, `Constrained[int]` and `Defaulted[int]` are all fully static,
# despite their bounds/constraints/defaults not being fully static
def returns_bounded(value: Any) -> Bounded[int]:
    return value  # error: [unsound-return-statement]

def returns_constrained(value: Any) -> Constrained[int]:
    return value  # error: [unsound-return-statement]

def returns_defaulted(value: Any) -> Defaulted[int]:
    return value  # error: [unsound-return-statement]
```

The same applies to classes declared with legacy type variables:

```py
BoundedT = TypeVar("BoundedT", bound=Any)
ConstrainedT = TypeVar("ConstrainedT", int, Any)
DefaultedT = TypeVar("DefaultedT", default=Any)

class LegacyBounded(Generic[BoundedT]): ...
class LegacyConstrained(Generic[ConstrainedT]): ...
class LegacyDefaulted(Generic[DefaultedT]): ...

def returns_legacy_bounded(value: Any) -> LegacyBounded[int]:
    return value  # error: [unsound-return-statement]

def returns_legacy_constrained(value: Any) -> LegacyConstrained[int]:
    return value  # error: [unsound-return-statement]

def returns_legacy_defaulted(value: Any) -> LegacyDefaulted[int]:
    return value  # error: [unsound-return-statement]
```

and to `return` statements in generator functions:

```py
def generator_returns_bounded(value: Any) -> Generator[None, None, Bounded[int]]:
    yield
    return value  # error: [unsound-return-statement]
```

A specialized generic type is nonetheless considered to be non-fully-static if it is specialized
with non-fully-static types:

```py
def returns_gradual_bounded(value: Any) -> Bounded[Any]:
    # no error
    return value

def returns_gradual_constrained(value: Any) -> Constrained[Any]:
    # no error
    return value

def returns_gradual_defaulted(value: Any) -> Defaulted[Any]:
    # no error
    return value

def returns_nested_gradual_bounded(value: Any) -> Bounded[list[Any]]:
    # no error
    return value
```

## Regression test: `unsound-return-statement` with tuple class objects

A tuple class has only one generic parameter, so its element types are combined into a union. Its
original element types must still determine whether the tuple class is fully static.

```toml
[environment]
python-version = "3.11"

[rules]
unsound-return-statement = "error"
```

A tuple class with fully static elements forms a fully static return boundary:

```py
from typing import Any

def returns_static_tuple_class(value: Any) -> type[tuple[int, object]]:
    return value  # error: [unsound-return-statement]
```

A tuple class with an `Any` element remains gradual even though the union `object | Any` simplifies
to `object`:

```py
def returns_gradual_tuple_class(value: Any) -> type[tuple[object, Any]]:
    return value
```

An unpacked gradual tuple likewise makes the entire tuple class gradual:

```py
def returns_gradual_variadic_tuple_class(value: Any) -> type[tuple[object, *tuple[Any, ...]]]:
    return value
```

## Regression test: `unsound-return-statement` uses "pure redundancy"

Internally, the rule uses "pure redundancy" rather than "impure redundancy". The following example
is a regression test that shows why this internal implementation detail is important. As an
optimisation as of 06 August 2026, `Phantom[str]` is not currently considered "impurely redundant"
with `Phantom[int]` (we do not simplify the union `Phantom[str] | Phantom[int]`). But the two
protocols are considered equivalent, are considered mutual subtypes of each other, and are
considered mutually redundant, meaning that no `unsound-return-statement` error is reported on this
snippet:

```toml
[rules]
unsound-return-statement = "error"
```

```py
from typing import Generator, Protocol, TypeVar

T = TypeVar("T")

class Phantom(Protocol[T]):
    def ping(self) -> int: ...

def returns_protocol(value: Phantom[int]) -> Phantom[str]:
    return value

def generator_returns_protocol(value: Phantom[int]) -> Generator[None, None, Phantom[str]]:
    yield
    return value
```

## Regression test: `unsound-return-statement` with non-fully-static `TypedDict`s

A `TypedDict` with a field or explicit extra items of type `Any` is not fully static, even when the
dictionary is defined as a class or inherits its fields from another `TypedDict`. The rule is not
applied to `TypedDict`s like this that are not fully static:

```toml
[rules]
unsound-return-statement = "error"
```

```py
from typing_extensions import Any, Generator, TypedDict

class StaticPayload(TypedDict):
    value: int

class DynamicPayload(TypedDict):
    value: Any

class InheritedDynamicPayload(DynamicPayload): ...
class DynamicExtraPayload(TypedDict, extra_items=Any): ...

FunctionalDynamicPayload = TypedDict("FunctionalDynamicPayload", {"value": Any})

def returns_dynamic_typed_dict(value: StaticPayload) -> DynamicPayload:
    return value

def returns_inherited_dynamic_typed_dict(value: StaticPayload) -> InheritedDynamicPayload:
    return value

def returns_functional_dynamic_typed_dict(value: StaticPayload) -> FunctionalDynamicPayload:
    return value

def returns_dynamic_extra_typed_dict(value: Any) -> DynamicExtraPayload:
    return value

def generator_returns_dynamic_typed_dict(
    value: StaticPayload,
) -> Generator[None, None, DynamicPayload]:
    yield
    return value

def returns_static_typed_dict(value: Any) -> StaticPayload:
    return value  # error: [unsound-return-statement]
```

## Regression test: `unsound-return-statement` + recursive structural types

Recursively specializing a protocol can produce infinitely many distinct types. Checking whether
such a return annotation is fully static must recognize the recurring protocol definition and
terminate instead of expanding the recursive member indefinitely, which would lead to a stack
overflow:

```toml
[rules]
unsound-return-statement = "error"
```

```py
from typing import Any, Protocol, TypeVar

T = TypeVar("T")

class Growing(Protocol[T]):
    @property
    def next(self) -> "Growing[list[T]]": ...

def returns_recursive_protocol(value: Any) -> Growing[int]:
    return value
```

The same protection is needed for class-based `TypedDict` fields that recursively specialize their
containing dictionary.

```py
from typing import Generic, TypedDict

class GrowingPayload(TypedDict, Generic[T]):
    child: "GrowingPayload[list[T]]"

def returns_recursive_typed_dict(value: Any) -> GrowingPayload[int]:
    return value
```

## Regression test: `unsound-return-statement` + recursive type aliases

Recursively specializing a generic type alias can also produce infinitely many distinct types. The
check for whether a type is fully static must recognize repeated visits to the same alias
definition, including when `Any` appears elsewhere in the recursive alias.

```toml
[environment]
python-version = "3.12"

[rules]
unsound-return-statement = "error"
```

```py
from typing import Any

type GrowingAlias[T] = list[GrowingAlias[list[T]]]
type GrowingAliasWithAny[T] = list[GrowingAliasWithAny[list[T]] | Any]

def returns_recursive_alias(value: Any) -> GrowingAlias[int]:
    return value

def returns_recursive_alias_with_any(value: Any) -> GrowingAliasWithAny[int]:
    return value
```
