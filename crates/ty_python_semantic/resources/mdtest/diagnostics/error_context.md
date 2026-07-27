# Error context for diagnostics involving assignability checks

```toml
[environment]
python-version = "3.13"
```

A lot of ty's diagnostics are emitted as a direct result of a type-to-type assignability check
(`invalid-assignment`, `invalid-argument-type` or `invalid-method-override`). Types can be complex,
and so we can often help users understand the incompatibility by focusing on the relevant parts of
the two types that are being compared.

## Basic

Mainly for comparison: this is the most basic kind of `invalid-assignment` diagnostic:

```py
def _(source: str):
    target: bytes = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str` is not assignable to `bytes`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: bytes = source  # snapshot
  |             -----   ^^^^^^ Incompatible value of type `str`
  |             |
  |             Declared type
  |
```

## Unions

Assigning a union to a non-union:

```py
def _(source: str | None):
    target: str = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str | None` is not assignable to `str`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: str = source  # snapshot
  |             ---   ^^^^^^ Incompatible value of type `str | None`
  |             |
  |             Declared type
  |
info: element `None` of union `str | None` is not assignable to `str`
```

Assigning a non-union to a union:

```py
def _(source: int):
    target: str | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `int` is not assignable to `str | None`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: str | None = source  # snapshot
  |             ----------   ^^^^^^ Incompatible value of type `int`
  |             |
  |             Declared type
  |
```

Assigning a union to a union:

```py
def _(source: str | None):
    target: bytes | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str | None` is not assignable to `bytes | None`
 --> src/mdtest_snippet.py:6:13
  |
6 |     target: bytes | None = source  # snapshot
  |             ------------   ^^^^^^ Incompatible value of type `str | None`
  |             |
  |             Declared type
  |
info: element `str` of union `str | None` is not assignable to `bytes | None`
```

## Intersections

Assigning an intersection to a non-intersection:

```py
from ty_extensions import Intersection
from typing import Protocol

class SupportsFoo(Protocol):
    def foo(self) -> None: ...

class SupportsBar(Protocol):
    def bar(self) -> None: ...

class SupportsFooAndBar(Protocol):
    def foo(self) -> None: ...
    def bar(self) -> None: ...

class HasFoo:
    def foo(self) -> None: ...

class HasBar:
    def bar(self) -> None: ...

class HasNeither: ...

def _(source: Intersection[HasBar, HasNeither]):
    target: SupportsFooAndBar = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasBar & HasNeither` is not assignable to `SupportsFooAndBar`
  --> src/mdtest_snippet.py:23:13
   |
23 |     target: SupportsFooAndBar = source  # snapshot
   |             -----------------   ^^^^^^ Incompatible value of type `HasBar & HasNeither`
   |             |
   |             Declared type
   |
info: no element of intersection `HasBar & HasNeither` is assignable to `SupportsFooAndBar`
info: ├── type `HasBar` is not assignable to protocol `SupportsFooAndBar`
info: │   └── protocol member `foo` is not defined on type `HasBar`
info: └── type `HasNeither` is not assignable to protocol `SupportsFooAndBar`
info:     └── protocol member `bar` is not defined on type `HasNeither`
```

Assigning a non-intersection to an intersection:

```py
def _(source: HasFoo):
    target: Intersection[SupportsFoo, SupportsBar] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasFoo` is not assignable to `SupportsFoo & SupportsBar`
  --> src/mdtest_snippet.py:25:13
   |
25 |     target: Intersection[SupportsFoo, SupportsBar] = source  # snapshot
   |             --------------------------------------   ^^^^^^ Incompatible value of type `HasFoo`
   |             |
   |             Declared type
   |
info: type `HasFoo` is not assignable to element `SupportsBar` of intersection `SupportsFoo & SupportsBar`
info: └── type `HasFoo` is not assignable to protocol `SupportsBar`
info:     └── protocol member `bar` is not defined on type `HasFoo`
```

Assigning an intersection to an intersection:

```py
def _(source: Intersection[HasFoo, HasNeither]):
    target: Intersection[SupportsFoo, SupportsBar] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasFoo & HasNeither` is not assignable to `SupportsFoo & SupportsBar`
  --> src/mdtest_snippet.py:27:13
   |
27 |     target: Intersection[SupportsFoo, SupportsBar] = source  # snapshot
   |             --------------------------------------   ^^^^^^ Incompatible value of type `HasFoo & HasNeither`
   |             |
   |             Declared type
   |
info: type `HasFoo & HasNeither` is not assignable to element `SupportsBar` of intersection `SupportsFoo & SupportsBar`
info: └── no element of intersection `HasFoo & HasNeither` is assignable to `SupportsBar`
info:     ├── type `HasFoo` is not assignable to protocol `SupportsBar`
info:     │   └── protocol member `bar` is not defined on type `HasFoo`
info:     └── type `HasNeither` is not assignable to protocol `SupportsBar`
info:         └── protocol member `bar` is not defined on type `HasNeither`
```

## Tuples

Wrong element types:

```py
def _(source: tuple[int, str, bool]):
    target: tuple[int, bytes, bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `tuple[int, str, bool]` is not assignable to `tuple[int, bytes, bool]`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: tuple[int, bytes, bool] = source  # snapshot
  |             -----------------------   ^^^^^^ Incompatible value of type `tuple[int, str, bool]`
  |             |
  |             Declared type
  |
info: the second tuple element is not compatible: `str` is not assignable to `bytes`
```

Wrong number of elements:

```py
def _(source: tuple[int, str]):
    target: tuple[int, str, bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `tuple[int, str]` is not assignable to `tuple[int, str, bool]`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: tuple[int, str, bool] = source  # snapshot
  |             ---------------------   ^^^^^^ Incompatible value of type `tuple[int, str]`
  |             |
  |             Declared type
  |
info: a tuple of length 2 is not assignable to a tuple of length 3
```

## `Callable`

Assigning a function to a `Callable`

```py
from typing import Any, Callable

def source(x: int, y: str) -> None:
    raise NotImplementedError

target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `def source(x: int, y: str) -> None` is not assignable to `(int, bytes, /) -> bool`
 --> src/mdtest_snippet.py:6:9
  |
6 | target: Callable[[int, bytes], bool] = source  # snapshot
  |         ----------------------------   ^^^^^^ Incompatible value of type `def source(x: int, y: str) -> None`
  |         |
  |         Declared type
  |
info: incompatible return types: `None` is not assignable to `bool`
```

Assigning a `Callable` to a `Callable` with wrong parameter type:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, str, /) -> bool` is not assignable to `(int, bytes, /) -> bool`
 --> src/mdtest_snippet.py:8:13
  |
8 |     target: Callable[[int, bytes], bool] = source  # snapshot
  |             ----------------------------   ^^^^^^ Incompatible value of type `(int, str, /) -> bool`
  |             |
  |             Declared type
  |
info: the second parameter has an incompatible type: `bytes` is not assignable to `str`
```

Assigning a `Callable` to a `Callable` with wrong return type:

```py
def _(source: Callable[[int, bytes], None]):
    target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, bytes, /) -> None` is not assignable to `(int, bytes, /) -> bool`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: Callable[[int, bytes], bool] = source  # snapshot
   |             ----------------------------   ^^^^^^ Incompatible value of type `(int, bytes, /) -> None`
   |             |
   |             Declared type
   |
info: incompatible return types: `None` is not assignable to `bool`
```

Assigning a `Callable` to a `Callable` with wrong number of parameters:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, str, /) -> bool` is not assignable to `(int, /) -> bool`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: Callable[[int], bool] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `(int, str, /) -> bool`
   |             |
   |             Declared type
   |
info: unexpected extra parameter
```

Assigning a function with an extra required parameter to a `Callable`:

```py
def source(x: int, extra: str) -> bool:
    raise NotImplementedError

target: Callable[[int], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `def source(x: int, extra: str) -> bool` is not assignable to `(int, /) -> bool`
  --> src/mdtest_snippet.py:16:9
   |
16 | target: Callable[[int], bool] = source  # snapshot
   |         ---------------------   ^^^^^^ Incompatible value of type `def source(x: int, extra: str) -> bool`
   |         |
   |         Declared type
   |
info: unexpected extra parameter `extra`
```

Assigning a class to a `Callable`

```py
class Number:
    def __init__(self, value: int): ...

target: Callable[[str], Any] = Number  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `<class 'Number'>` is not assignable to `(str, /) -> Any`
  --> src/mdtest_snippet.py:20:9
   |
20 | target: Callable[[str], Any] = Number  # snapshot
   |         --------------------   ^^^^^^ Incompatible value of type `<class 'Number'>`
   |         |
   |         Declared type
   |
info: type `<class 'Number'>` has inferred callable type `(value: int) -> Number`
info: └── the first parameter has an incompatible type: `str` is not assignable to `int`
```

Passing a class to a function expecting a `Callable`:

```py
from typing import Any, Callable

def accepts_callable(callback: Callable[[Any], Any]) -> None: ...

class Foo:
    def __init__(self, x: Any, y: Any): ...

accepts_callable(Foo)  # snapshot
```

```snapshot
error[invalid-argument-type]: Argument to function `accepts_callable` is incorrect
  --> src/mdtest_snippet.py:28:18
   |
28 | accepts_callable(Foo)  # snapshot
   |                  ^^^ Expected `(Any, /) -> Any`, found `<class 'Foo'>`
   |
info: type `<class 'Foo'>` has inferred callable type `(x: Any, y: Any) -> Foo`
info: └── unexpected extra parameter `y`
info: Function defined here
  --> src/mdtest_snippet.py:23:5
   |
23 | def accepts_callable(callback: Callable[[Any], Any]) -> None: ...
   |     ^^^^^^^^^^^^^^^^ ------------------------------ Parameter declared here
   |
```

Assigning a bound method to a `Callable`:

```py
class Greeter:
    def greet(self, name: str, greeting: str = "Hello") -> str:
        return f"{greeting}, {name}"

greeter = Greeter()
bound_method_target: Callable[[int], str] = greeter.greet  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `bound method Greeter.greet(name: str, greeting: str = "Hello") -> str` is not assignable to `(int, /) -> str`
  --> src/mdtest_snippet.py:34:22
   |
34 | bound_method_target: Callable[[int], str] = greeter.greet  # snapshot
   |                      --------------------   ^^^^^^^^^^^^^ Incompatible value of type `bound method Greeter.greet(name: str, greeting: str = "Hello") -> str`
   |                      |
   |                      Declared type
   |
info: the first parameter has an incompatible type: `int` is not assignable to `str`
```

Assigning a known bound method to a `Callable`:

```py
def callable_base(x: int) -> bool:
    return True

known_bound_method_target: Callable[[str], bool] = callable_base.__call__  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `<method-wrapper '__call__' of function 'callable_base'>` is not assignable to `(str, /) -> bool`
  --> src/mdtest_snippet.py:38:28
   |
38 | known_bound_method_target: Callable[[str], bool] = callable_base.__call__  # snapshot
   |                            ---------------------   ^^^^^^^^^^^^^^^^^^^^^^ Incompatible value of type `<method-wrapper '__call__' of function 'callable_base'>`
   |                            |
   |                            Declared type
   |
info: type `<method-wrapper '__call__' of function 'callable_base'>` has inferred callable type `(x: int) -> bool`
info: └── the first parameter has an incompatible type: `str` is not assignable to `int`
```

Assigning a `functools.partial` result to a `Callable`:

```py
from functools import partial

def predicate(x: int, y: str) -> bool:
    return True

partial_predicate = partial(predicate, 1)
partial_target: Callable[[bytes], bool] = partial_predicate  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `partial[(y: str) -> bool]` is not assignable to `(bytes, /) -> bool`
  --> src/mdtest_snippet.py:45:17
   |
45 | partial_target: Callable[[bytes], bool] = partial_predicate  # snapshot
   |                 -----------------------   ^^^^^^^^^^^^^^^^^ Incompatible value of type `partial[(y: str) -> bool]`
   |                 |
   |                 Declared type
   |
info: the first parameter has an incompatible type: `bytes` is not assignable to `str`
```

## Function assignability and overrides

Liskov checks use function-to-function assignability.

Wrong parameter type:

```py
class Parent:
    def method(self, x: str) -> bool:
        raise NotImplementedError

class Child1(Parent):
    # snapshot
    def method(self, x: bytes) -> bool:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
 --> src/mdtest_snippet.py:7:9
  |
7 |     def method(self, x: bytes) -> bool:
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
  |
 ::: src/mdtest_snippet.py:2:9
  |
2 |     def method(self, x: str) -> bool:
  |         ---------------------------- `Parent.method` defined here
  |
info: parameter `x` has an incompatible type: `str` is not assignable to `bytes`
info: This violates the Liskov Substitution Principle
```

We call out the correct (target) parameter if they are listed in a different order:

```py
class ParentXY:
    def method(self, *, x: str, y: int) -> bool:
        raise NotImplementedError

class ChildYX(ParentXY):
    # snapshot
    def method(self, *, y: int, x: bytes) -> bool:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:15:9
   |
15 |     def method(self, *, y: int, x: bytes) -> bool:
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `ParentXY.method`
   |
  ::: src/mdtest_snippet.py:10:9
   |
10 |     def method(self, *, x: str, y: int) -> bool:
   |         --------------------------------------- `ParentXY.method` defined here
   |
info: parameter `x` has an incompatible type: `str` is not assignable to `bytes`
info: This violates the Liskov Substitution Principle
```

Wrong return type:

```py
class Child2(Parent):
    # snapshot
    def method(self, x: str) -> None:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:19:9
   |
19 |     def method(self, x: str) -> None:
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/mdtest_snippet.py:2:9
   |
 2 |     def method(self, x: str) -> bool:
   |         ---------------------------- `Parent.method` defined here
   |
info: incompatible return types: `None` is not assignable to `bool`
info: This violates the Liskov Substitution Principle
```

Wrong non-positional-only parameter name:

```py
class Child3(Parent):
    # snapshot
    def method(self, y: str):
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:23:9
   |
23 |     def method(self, y: str):
   |         ^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/mdtest_snippet.py:2:9
   |
 2 |     def method(self, x: str) -> bool:
   |         ---------------------------- `Parent.method` defined here
   |
info: the parameter named `y` does not match `x` (and can be used as a keyword parameter)
info: This violates the Liskov Substitution Principle
```

## `TypedDict`

Incompatible field types:

```py
from typing import Any, TypedDict, NotRequired, ReadOnly

class Person(TypedDict):
    name: str

class Other(TypedDict):
    name: bytes

def _(source: Person):
    target: Other = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `Other`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: Other = source  # snapshot
   |             -----   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
info: field "name" on TypedDict `Person` has type `str` which is not assignable to type `bytes` expected by TypedDict `Other`
```

Missing required fields:

```py
class PersonWithAge(TypedDict):
    name: str
    age: int

def _(source: Person):
    target: PersonWithAge = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `PersonWithAge`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: PersonWithAge = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
info: required field "age" is not present in source TypedDict `Person`
```

Non-required fields that are required in the target:

```py
class PersonWithOptionalAge(TypedDict):
    name: str
    age: NotRequired[int]

def _(source: PersonWithOptionalAge):
    target: PersonWithAge = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `PersonWithOptionalAge` is not assignable to `PersonWithAge`
  --> src/mdtest_snippet.py:22:13
   |
22 |     target: PersonWithAge = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `PersonWithOptionalAge`
   |             |
   |             Declared type
   |
info: field "age" is required in TypedDict `PersonWithAge` but not required in TypedDict `PersonWithOptionalAge`
```

Read-only fields that are mutable in the target:

```py
class PersonWithReadOnlyName(TypedDict):
    name: ReadOnly[str]

def _(source: PersonWithReadOnlyName):
    target: Person = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `PersonWithReadOnlyName` is not assignable to `Person`
  --> src/mdtest_snippet.py:27:13
   |
27 |     target: Person = source  # snapshot
   |             ------   ^^^^^^ Incompatible value of type `PersonWithReadOnlyName`
   |             |
   |             Declared type
   |
info: field "name" is read-only in TypedDict `PersonWithReadOnlyName` but mutable in TypedDict `Person`
```

Required fields that are not required and mutable in the target:

```py
def _(source: PersonWithAge):
    target: PersonWithOptionalAge = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `PersonWithAge` is not assignable to `PersonWithOptionalAge`
  --> src/mdtest_snippet.py:29:13
   |
29 |     target: PersonWithOptionalAge = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `PersonWithAge`
   |             |
   |             Declared type
   |
info: field "age" is required in TypedDict `PersonWithAge` but not required and mutable in TypedDict `PersonWithOptionalAge`
help: The required field could be removed through a destructive operation like `del` on the target.
```

Assigning a `TypedDict` to a `dict`

```py
def _(source: Person):
    target: dict[str, Any] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `dict[str, Any]`
  --> src/mdtest_snippet.py:31:13
   |
31 |     target: dict[str, Any] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
info: TypedDict `Person` is not assignable to `dict`
help: A TypedDict is not usually assignable to any `dict[..]` type; `dict` types allow destructive operations like `clear()`.
help: Consider using `Mapping[..]` instead of `dict[..]`.
```

## Generic `TypedDict` field conflicts in overload diagnostics

A generic `TypedDict` relation can be unsatisfiable without being the `never` terminal. The
resulting overload diagnostic should still explain which field introduced the conflicting
constraints.

```py
from typing import Generic, Self, TypeVar, TypedDict, overload

T = TypeVar("T")

class Pair(TypedDict, Generic[T]):
    first: T
    second: T

class Fixed(TypedDict):
    first: int
    second: str

class OverloadedSelf:
    @overload
    def method(self, value: Fixed) -> None: ...  # snapshot: invalid-overload
    @overload
    def method(self, value: str) -> None: ...
    def method(self, value: Pair[Self] | str) -> None: ...
```

```snapshot
error[invalid-overload]: Implementation does not accept all arguments of this overload
  --> src/mdtest_snippet.py:15:9
   |
15 |     def method(self, value: Fixed) -> None: ...  # snapshot: invalid-overload
   |         ^^^^^^
16 |     @overload
17 |     def method(self, value: str) -> None: ...
18 |     def method(self, value: Pair[Self] | str) -> None: ...
   |         ------ Implementation defined here
   |
info: Implementation signature `(self, value: Pair[Self@method] | str) -> None` is not assignable to overload signature `(self, value: Fixed) -> None`
info: parameter `value` has an incompatible type: `Fixed` is not assignable to `Pair[Self@method] | str`
info: └── type `Fixed` is not assignable to any element of the union `Pair[Self@method] | str`
info:     ├── field "second" on TypedDict `Fixed` has type `str` which is not assignable to type `Self@method` expected by TypedDict `Pair`
info:     └── ... omitted 1 union element without additional context
```

## Stop checking callable parameters after incompatible generic constraints

Once earlier parameters produce an unsatisfiable nonterminal constraint set, continuing to a later
parameter must not replace the diagnostic context that explains the original incompatibility.

```py
from typing import Generic, Self, TypeVar, TypedDict, overload

T = TypeVar("T")

class Pair(TypedDict, Generic[T]):
    first: T
    second: T

class Fixed(TypedDict):
    first: int
    second: str

class OverloadedSelf:
    @overload
    def method(self, value: Fixed, later: int) -> None: ...  # snapshot: invalid-overload
    @overload
    def method(self, value: str, later: str) -> None: ...
    def method(self, value: Pair[Self] | str, later: str) -> None: ...
```

```snapshot
error[invalid-overload]: Implementation does not accept all arguments of this overload
  --> src/mdtest_snippet.py:15:9
   |
15 |     def method(self, value: Fixed, later: int) -> None: ...  # snapshot: invalid-overload
   |         ^^^^^^
16 |     @overload
17 |     def method(self, value: str, later: str) -> None: ...
18 |     def method(self, value: Pair[Self] | str, later: str) -> None: ...
   |         ------ Implementation defined here
   |
info: Implementation signature `(self, value: Pair[Self@method] | str, later: str) -> None` is not assignable to overload signature `(self, value: Fixed, later: int) -> None`
info: parameter `value` has an incompatible type: `Fixed` is not assignable to `Pair[Self@method] | str`
info: └── type `Fixed` is not assignable to any element of the union `Pair[Self@method] | str`
info:     ├── field "second" on TypedDict `Fixed` has type `str` which is not assignable to type `Self@method` expected by TypedDict `Pair`
info:     └── ... omitted 1 union element without additional context
```

## Type variable upper bounds

Assignability context is included when an explicit type argument does not satisfy a type variable's
upper bound:

```py
from typing import Generic, TypeVar

T = TypeVar("T", bound=tuple[int, bytes, bool])

class Box(Generic[T]): ...

bad: Box[tuple[int, str, bool]]  # snapshot: invalid-type-arguments
```

```snapshot
error[invalid-type-arguments]: Type `tuple[int, str, bool]` is not assignable to upper bound `tuple[int, bytes, bool]` of type variable `T@Box`
 --> src/mdtest_snippet.py:3:1
  |
3 | T = TypeVar("T", bound=tuple[int, bytes, bool])
  | - Type variable defined here
4 |
5 | class Box(Generic[T]): ...
6 |
7 | bad: Box[tuple[int, str, bool]]  # snapshot: invalid-type-arguments
  |          ^^^^^^^^^^^^^^^^^^^^^
  |
info: the second tuple element is not compatible: `str` is not assignable to `bytes`
```

## Protocols

Missing protocol members:

```py
from typing import Protocol

class SupportsCheck(Protocol):
    def check(self, x: int, y: str) -> bool: ...

class DoesNotHaveCheck: ...

def _(source: DoesNotHaveCheck):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotHaveCheck` is not assignable to `SupportsCheck`
 --> src/mdtest_snippet.py:9:13
  |
9 |     target: SupportsCheck = source  # snapshot
  |             -------------   ^^^^^^ Incompatible value of type `DoesNotHaveCheck`
  |             |
  |             Declared type
  |
info: type `DoesNotHaveCheck` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is not defined on type `DoesNotHaveCheck`
```

Incompatible types for protocol members:

```py
class CheckWithWrongSignature:
    def check(self, x: int, y: bytes) -> bool:
        return False

def _(source: CheckWithWrongSignature):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `CheckWithWrongSignature` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:15:13
   |
15 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `CheckWithWrongSignature`
   |             |
   |             Declared type
   |
info: type `CheckWithWrongSignature` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is incompatible
info:     └── parameter `y` has an incompatible type: `str` is not assignable to `bytes`
```

Missing protocol properties:

```py
class SupportsName(Protocol):
    @property
    def name(self) -> str: ...

class DoesNotHaveName: ...

def _(source: DoesNotHaveName):
    target: SupportsName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotHaveName` is not assignable to `SupportsName`
  --> src/mdtest_snippet.py:23:13
   |
23 |     target: SupportsName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `DoesNotHaveName`
   |             |
   |             Declared type
   |
info: type `DoesNotHaveName` is not assignable to protocol `SupportsName`
info: └── protocol member `name` is not defined on type `DoesNotHaveName`
```

Missing protocol members (protocol to protocol):

```py
class SupportsSomethingElse(Protocol):
    def something_else(self) -> None: ...

def _(source: SupportsSomethingElse):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `SupportsSomethingElse` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:28:13
   |
28 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `SupportsSomethingElse`
   |             |
   |             Declared type
   |
info: protocol `SupportsSomethingElse` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is not defined on type `SupportsSomethingElse`
```

Incompatible readable and writable protocol attributes:

```py
from typing import Protocol

class ReadableName(Protocol):
    @property
    def name(self) -> str: ...

class WritableName(Protocol):
    name: str

class BytesName:
    name: bytes

class ReadOnlyName:
    @property
    def name(self) -> str:
        return ""

class BytesSetterName:
    @property
    def name(self) -> str:
        return ""

    @name.setter
    def name(self, value: bytes) -> None: ...
```

```py
def _(source: BytesName):
    target: ReadableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `BytesName` is not assignable to `ReadableName`
  --> src/mdtest_snippet.py:54:13
   |
54 |     target: ReadableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `BytesName`
   |             |
   |             Declared type
   |
info: type `BytesName` is not assignable to protocol `ReadableName`
info: └── protocol member `name` is incompatible
info:     └── read type `bytes` is not assignable to `str`
```

```py
def _(source: ReadOnlyName):
    target: WritableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `ReadOnlyName` is not assignable to `WritableName`
  --> src/mdtest_snippet.py:56:13
   |
56 |     target: WritableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `ReadOnlyName`
   |             |
   |             Declared type
   |
info: type `ReadOnlyName` is not assignable to protocol `WritableName`
info: └── protocol member `name` is incompatible
info:     └── the member does not accept writes of type `str`
```

```py
def _(source: BytesSetterName):
    target: WritableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `BytesSetterName` is not assignable to `WritableName`
  --> src/mdtest_snippet.py:58:13
   |
58 |     target: WritableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `BytesSetterName`
   |             |
   |             Declared type
   |
info: type `BytesSetterName` is not assignable to protocol `WritableName`
info: └── protocol member `name` is incompatible
info:     └── the member does not accept writes of type `str`
```

Incompatible readable and writable attributes when assigning one protocol to another:

```py
class ReadOnlyNameProtocol(Protocol):
    @property
    def name(self) -> str: ...

class BytesNameProtocol(Protocol):
    name: bytes

class BytesSetterNameProtocol(Protocol):
    @property
    def name(self) -> str: ...
    @name.setter
    def name(self, value: bytes) -> None: ...
```

```py
def _(source: ReadOnlyNameProtocol):
    target: WritableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `ReadOnlyNameProtocol` is not assignable to `WritableName`
  --> src/mdtest_snippet.py:72:13
   |
72 |     target: WritableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `ReadOnlyNameProtocol`
   |             |
   |             Declared type
   |
info: protocol `ReadOnlyNameProtocol` is not assignable to protocol `WritableName`
info: └── protocol member `name` is incompatible
info:     └── the member is not writable
```

```py
def _(source: BytesNameProtocol):
    target: WritableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `BytesNameProtocol` is not assignable to `WritableName`
  --> src/mdtest_snippet.py:74:13
   |
74 |     target: WritableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `BytesNameProtocol`
   |             |
   |             Declared type
   |
info: protocol `BytesNameProtocol` is not assignable to protocol `WritableName`
info: └── protocol member `name` is incompatible
info:     └── read type `bytes` is not assignable to `str`
```

```py
def _(source: BytesSetterNameProtocol):
    target: WritableName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `BytesSetterNameProtocol` is not assignable to `WritableName`
  --> src/mdtest_snippet.py:76:13
   |
76 |     target: WritableName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `BytesSetterNameProtocol`
   |             |
   |             Declared type
   |
info: protocol `BytesSetterNameProtocol` is not assignable to protocol `WritableName`
info: └── protocol member `name` is incompatible
info:     └── the member does not accept writes of type `str`
```

Incompatible types for protocol members (protocol to protocol):

```py
class SupportsCheckWithOtherSignature(Protocol):
    def check(self, x: int, y: bytes) -> bool: ...

def _(source: SupportsCheckWithOtherSignature):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `SupportsCheckWithOtherSignature` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:81:13
   |
81 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `SupportsCheckWithOtherSignature`
   |             |
   |             Declared type
   |
info: protocol `SupportsCheckWithOtherSignature` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is incompatible
info:     └── parameter `y` has an incompatible type: `str` is not assignable to `bytes`
```

## Type aliases

Type aliases should be expanded in diagnostics to understand the underlying incompatibilities:

```py
from typing import Protocol

class SupportsName(Protocol):
    def name(self) -> str: ...

class HasName:
    def name(self) -> bytes:
        return b""

type StringOrName = str | SupportsName

def _(source: HasName):
    target: StringOrName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasName` is not assignable to `StringOrName`
  --> src/mdtest_snippet.py:13:13
   |
13 |     target: StringOrName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `HasName`
   |             |
   |             Declared type
   |
info: type `HasName` is not assignable to any element of the union `str | SupportsName`
info: ├── type `HasName` is not assignable to protocol `SupportsName`
info: │   └── protocol member `name` is incompatible
info: │       └── incompatible return types: `bytes` is not assignable to `str`
info: └── ... omitted 1 union element without additional context
```

## Deeply nested incompatibilities

```py
from typing import Callable

def source(x: tuple[int, str]) -> bool:
    return False

target: Callable[[tuple[int, bytes]], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `def source(x: tuple[int, str]) -> bool` is not assignable to `(tuple[int, bytes], /) -> bool`
 --> src/mdtest_snippet.py:6:9
  |
6 | target: Callable[[tuple[int, bytes]], bool] = source  # snapshot
  |         -----------------------------------   ^^^^^^ Incompatible value of type `def source(x: tuple[int, str]) -> bool`
  |         |
  |         Declared type
  |
info: the first parameter has an incompatible type: `tuple[int, bytes]` is not assignable to `tuple[int, str]`
info: └── the second tuple element is not compatible: `bytes` is not assignable to `str`
```

## Multiple nested incompatibilities

```py
from typing import Protocol

class SupportsCheck(Protocol):
    def check1(self, x: str): ...
    def check2(self, x: int) -> bool: ...

class Incompatible:
    def check1(self, x: bytes): ...
    def check2(self, x: int) -> None: ...

def _(source: Incompatible):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Incompatible` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `Incompatible`
   |             |
   |             Declared type
   |
info: type `Incompatible` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check1` is incompatible
info:     └── parameter `x` has an incompatible type: `str` is not assignable to `bytes`
```

## Failures for multiple union elements

```py
from typing import Protocol

class SupportsFoo(Protocol):
    def foo(self, x: int) -> bool: ...

class SupportsBar(Protocol):
    def bar(self, x: str) -> bool: ...

class HasNeither: ...

def _(source: HasNeither):
    target: SupportsFoo | SupportsBar = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasNeither` is not assignable to `SupportsFoo | SupportsBar`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: SupportsFoo | SupportsBar = source  # snapshot
   |             -------------------------   ^^^^^^ Incompatible value of type `HasNeither`
   |             |
   |             Declared type
   |
info: type `HasNeither` is not assignable to any element of the union `SupportsFoo | SupportsBar`
info: ├── type `HasNeither` is not assignable to protocol `SupportsFoo`
info: │   └── protocol member `foo` is not defined on type `HasNeither`
info: └── type `HasNeither` is not assignable to protocol `SupportsBar`
info:     └── protocol member `bar` is not defined on type `HasNeither`
```

## Failures for many union elements

```py
def _(source: int):
    target: str | bytes | bool | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `int` is not assignable to `str | bytes | bool | None`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: str | bytes | bool | None = source  # snapshot
  |             -------------------------   ^^^^^^ Incompatible value of type `int`
  |             |
  |             Declared type
  |
```

## Failures for multiple intersection elements

```py
from ty_extensions import Intersection
from typing import Protocol

class SupportsFoo(Protocol):
    def foo(self, x: int) -> bool: ...

class DoesNotSupportFoo1: ...
class DoesNotSupportFoo2: ...

def _(source: Intersection[DoesNotSupportFoo1, DoesNotSupportFoo2]):
    target: SupportsFoo = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotSupportFoo1 & DoesNotSupportFoo2` is not assignable to `SupportsFoo`
  --> src/mdtest_snippet.py:11:13
   |
11 |     target: SupportsFoo = source  # snapshot
   |             -----------   ^^^^^^ Incompatible value of type `DoesNotSupportFoo1 & DoesNotSupportFoo2`
   |             |
   |             Declared type
   |
info: no element of intersection `DoesNotSupportFoo1 & DoesNotSupportFoo2` is assignable to `SupportsFoo`
info: ├── type `DoesNotSupportFoo1` is not assignable to protocol `SupportsFoo`
info: │   └── protocol member `foo` is not defined on type `DoesNotSupportFoo1`
info: └── type `DoesNotSupportFoo2` is not assignable to protocol `SupportsFoo`
info:     └── protocol member `foo` is not defined on type `DoesNotSupportFoo2`
```

## Assigning an overload set

This test makes sure that error context from failed overload candidates does not leak
(`IncompatibleFoo.bar` is assignable to `SupportsFooAndBar.bar`):

```py
from typing import Protocol, overload, SupportsIndex

class SupportsFooAndBar(Protocol):
    def foo(self, name: str): ...
    def bar(self, x: bytes): ...

class IncompatibleFoo:
    def foo(self, name_: str): ...
    @overload
    def bar(self, x: SupportsIndex): ...
    @overload
    def bar(self, x: bytes): ...
    def bar(self, x: SupportsIndex | bytes): ...

def _(source: IncompatibleFoo):
    target: SupportsFooAndBar = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `IncompatibleFoo` is not assignable to `SupportsFooAndBar`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: SupportsFooAndBar = source  # snapshot
   |             -----------------   ^^^^^^ Incompatible value of type `IncompatibleFoo`
   |             |
   |             Declared type
   |
info: type `IncompatibleFoo` is not assignable to protocol `SupportsFooAndBar`
info: └── protocol member `foo` is incompatible
info:     └── the parameter named `name_` does not match `name` (and can be used as a keyword parameter)
```

## Assigning to `Iterable`

```py
from collections.abc import Iterable

def _(source: list[str]):
    target: Iterable[bytes] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[str]` is not assignable to `Iterable[bytes]`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: Iterable[bytes] = source  # snapshot
  |             ---------------   ^^^^^^ Incompatible value of type `list[str]`
  |             |
  |             Declared type
  |
info: type `list[str]` is not assignable to protocol `Iterable[bytes]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[str]` is not assignable to `Iterator[bytes]`
info:         └── protocol `Iterator[str]` is not assignable to protocol `Iterator[bytes]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `str` is not assignable to `bytes`
```

## Invariant generic classes

We show a special diagnostic hint for invariant generic classes. For example, if you try to assign a
`list[bool]` to a `list[int]`:

```py
def _(source: list[bool]):
    target: list[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[bool]` is not assignable to `list[int]`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: list[int] = source  # snapshot
  |             ---------   ^^^^^^ Incompatible value of type `list[bool]`
  |             |
  |             Declared type
  |
info: `list` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Sequence`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We do the same for other invariant generic classes:

```py
from collections import ChainMap, Counter, OrderedDict, defaultdict, deque
from collections.abc import MutableSequence, MutableMapping, MutableSet

def _(source: set[bool]):
    target: set[int] = source  # snapshot

def _(source: dict[str, bool]):
    target: dict[str, int] = source  # snapshot

def _(source: dict[bool, str]):
    target: dict[int, str] = source  # snapshot

def _(source: dict[bool, bool]):
    target: dict[int, int] = source  # snapshot

def _(source: defaultdict[str, bool]):
    target: defaultdict[str, int] = source  # snapshot

def _(source: defaultdict[bool, str]):
    target: defaultdict[int, str] = source  # snapshot

def _(source: OrderedDict[str, bool]):
    target: OrderedDict[str, int] = source  # snapshot

def _(source: OrderedDict[bool, str]):
    target: OrderedDict[int, str] = source  # snapshot

def _(source: ChainMap[str, bool]):
    target: ChainMap[str, int] = source  # snapshot

def _(source: ChainMap[bool, str]):
    target: ChainMap[int, str] = source  # snapshot

def _(source: deque[bool]):
    target: deque[int] = source  # snapshot

def _(source: Counter[bool]):
    target: Counter[int] = source  # snapshot

def _(source: MutableSequence[bool]):
    target: MutableSequence[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `set[bool]` is not assignable to `set[int]`
 --> src/mdtest_snippet.py:7:13
  |
7 |     target: set[int] = source  # snapshot
  |             --------   ^^^^^^ Incompatible value of type `set[bool]`
  |             |
  |             Declared type
  |
info: `set` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Set`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[str, bool]` is not assignable to `dict[str, int]`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: dict[str, int] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[str, bool]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[bool, str]` is not assignable to `dict[int, str]`
  --> src/mdtest_snippet.py:13:13
   |
13 |     target: dict[int, str] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[bool, str]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[bool, bool]` is not assignable to `dict[int, int]`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: dict[int, int] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[bool, bool]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its first and second type parameters
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `defaultdict[str, bool]` is not assignable to `defaultdict[str, int]`
  --> src/mdtest_snippet.py:19:13
   |
19 |     target: defaultdict[str, int] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `defaultdict[str, bool]`
   |             |
   |             Declared type
   |
info: `defaultdict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `defaultdict[bool, str]` is not assignable to `defaultdict[int, str]`
  --> src/mdtest_snippet.py:22:13
   |
22 |     target: defaultdict[int, str] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `defaultdict[bool, str]`
   |             |
   |             Declared type
   |
info: `defaultdict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `OrderedDict[str, bool]` is not assignable to `OrderedDict[str, int]`
  --> src/mdtest_snippet.py:25:13
   |
25 |     target: OrderedDict[str, int] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `OrderedDict[str, bool]`
   |             |
   |             Declared type
   |
info: `OrderedDict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `OrderedDict[bool, str]` is not assignable to `OrderedDict[int, str]`
  --> src/mdtest_snippet.py:28:13
   |
28 |     target: OrderedDict[int, str] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `OrderedDict[bool, str]`
   |             |
   |             Declared type
   |
info: `OrderedDict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `ChainMap[str, bool]` is not assignable to `ChainMap[str, int]`
  --> src/mdtest_snippet.py:31:13
   |
31 |     target: ChainMap[str, int] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `ChainMap[str, bool]`
   |             |
   |             Declared type
   |
info: `ChainMap` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `ChainMap[bool, str]` is not assignable to `ChainMap[int, str]`
  --> src/mdtest_snippet.py:34:13
   |
34 |     target: ChainMap[int, str] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `ChainMap[bool, str]`
   |             |
   |             Declared type
   |
info: `ChainMap` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `deque[bool]` is not assignable to `deque[int]`
  --> src/mdtest_snippet.py:37:13
   |
37 |     target: deque[int] = source  # snapshot
   |             ----------   ^^^^^^ Incompatible value of type `deque[bool]`
   |             |
   |             Declared type
   |
info: `deque` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Sequence`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `Counter[bool]` is not assignable to `Counter[int]`
  --> src/mdtest_snippet.py:40:13
   |
40 |     target: Counter[int] = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `Counter[bool]`
   |             |
   |             Declared type
   |
info: `Counter` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `MutableSequence[bool]` is not assignable to `MutableSequence[int]`
  --> src/mdtest_snippet.py:43:13
   |
43 |     target: MutableSequence[int] = source  # snapshot
   |             --------------------   ^^^^^^ Incompatible value of type `MutableSequence[bool]`
   |             |
   |             Declared type
   |
info: `MutableSequence` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We also show this hint for custom invariant generic classes:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class MyContainer(Generic[T]):
    value: T

def _(source: MyContainer[bool]):
    target: MyContainer[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `MyContainer[bool]` is not assignable to `MyContainer[int]`
  --> src/mdtest_snippet.py:52:13
   |
52 |     target: MyContainer[int] = source  # snapshot
   |             ----------------   ^^^^^^ Incompatible value of type `MyContainer[bool]`
   |             |
   |             Declared type
   |
info: `MyContainer` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We do *not* show this hint if the element types themselves wouldn't be assignable:

```py
def _(source: list[int]):
    target: list[str] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[int]` is not assignable to `list[str]`
  --> src/mdtest_snippet.py:54:13
   |
54 |     target: list[str] = source  # snapshot
   |             ---------   ^^^^^^ Incompatible value of type `list[int]`
   |             |
   |             Declared type
   |
```

We do not emit any error if the collection types are covariant:

```py
from collections.abc import Sequence

def _(source: list[bool]):
    target: Sequence[int] = source

def _(source: frozenset[bool]):
    target: frozenset[int] = source

def _(source: tuple[bool, bool]):
    target: tuple[int, int] = source
```

## Error context in other scenarios

### In `invalid-return-type` diagnostics

```py
def f() -> tuple[int, str]:
    return 1, b""  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:1:12
  |
1 | def f() -> tuple[int, str]:
  |            --------------- Expected `tuple[int, str]` because of return type
2 |     return 1, b""  # snapshot: invalid-return-type
  |            ^^^^^^ expected `tuple[int, str]`, found `tuple[Literal[1], Literal[b""]]`
  |
info: the second tuple element is not compatible: `Literal[b""]` is not assignable to `str`
```

### In `invalid-assignment` for attribute assignments

```py
class C:
    x: tuple[int, str]

c = C()
c.x = (1, b"")  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `tuple[Literal[1], Literal[b""]]` is not assignable to attribute `x` of type `tuple[int, str]`
 --> src/mdtest_snippet.py:5:1
  |
5 | c.x = (1, b"")  # snapshot
  | ^^^
  |
info: the second tuple element is not compatible: `Literal[b""]` is not assignable to `str`
```

### In `invalid-yield` diagnostics

```py
from typing import Generator

def f() -> Generator[tuple[int, str], None, None]:
    yield (1, b"")  # snapshot: invalid-yield
```

```snapshot
error[invalid-yield]: Yield expression type does not match annotation
 --> src/mdtest_snippet.py:3:12
  |
3 | def f() -> Generator[tuple[int, str], None, None]:
  |            -------------------------------------- Function annotated with yield type `tuple[int, str]` here
4 |     yield (1, b"")  # snapshot: invalid-yield
  |           ^^^^^^^^ expression of type `tuple[Literal[1], Literal[b""]]`, expected `tuple[int, str]`
  |
info: the second tuple element is not compatible: `Literal[b""]` is not assignable to `str`
```

### In `not-iterable` diagnostics

```py
from typing import Iterable, Iterator, Self

class WrongIterator:
    def __next__(self, wrong: str) -> int:
        return 0

class WrongIterable:
    def __iter__(self) -> WrongIterator:
        return WrongIterator()

# snapshot: not-iterable
for _ in WrongIterable():
    pass
```

```snapshot
error[not-iterable]: Object of type `WrongIterable` is not iterable
  --> src/mdtest_snippet.py:12:10
   |
12 | for _ in WrongIterable():
   |          ^^^^^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `WrongIterator`, which has an invalid `__next__` method
info: type `WrongIterable` is not assignable to protocol `Iterable[Unknown]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `WrongIterator` is not assignable to `Iterator[Unknown]`
info:         └── type `WrongIterator` is not assignable to protocol `Iterator[Unknown]`
info:             └── protocol member `__next__` is incompatible
info:                 └── unexpected extra parameter `wrong`
info: Expected signature for `__next__` is `def __next__(self): ...`
```
