# Comparison: Membership Test

In Python, the term "membership test operators" refers to the operators `in` and `not in`. To
customize their behavior, classes can implement one of the special methods `__contains__`,
`__iter__`, or `__getitem__`.

For references, see:

- <https://docs.python.org/3/reference/expressions.html#membership-test-details>
- <https://docs.python.org/3/reference/datamodel.html#object.__contains__>
- <https://snarky.ca/unravelling-membership-testing/>

## Implements `__contains__`

Classes can support membership tests by implementing the `__contains__` method:

```py
class A:
    def __contains__(self, item: str) -> bool:
        return True

reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
# error: [unsupported-operator] "Operator `in` is not supported between objects of type `Literal[42]` and `A`"
reveal_type(42 in A())  # revealed: bool
# error: [unsupported-operator] "Operator `not in` is not supported between objects of type `Literal[42]` and `A`"
reveal_type(42 not in A())  # revealed: bool
```

## Implements `__iter__`

Classes that don't implement `__contains__`, but do implement `__iter__`, also support containment
checks; the needle will be sought in their iterated items:

```py
class StringIterator:
    def __next__(self) -> str:
        return "foo"

class A:
    def __iter__(self) -> StringIterator:
        return StringIterator()

reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

## Implements `__getitems__`

The final fallback is to implement `__getitem__` for integer keys. Python will call `__getitem__`
with `0`, `1`, `2`... until either the needle is found (leading the membership test to evaluate to
`True`) or `__getitem__` raises `IndexError` (the raised exception is swallowed, but results in the
membership test evaluating to `False`).

```py
class A:
    def __getitem__(self, key: int) -> str:
        return "foo"

reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

## `__contains__` implemented via descriptor

If `__contains__` is implemented as a descriptor (e.g., a class with `__get__` that returns a
callable), the descriptor protocol should be properly invoked:

```py
class Target:
    def __call__(self, item: object) -> bool:
        return True

class Descriptor:
    def __get__(self, instance: object, owner: type) -> Target:
        return Target()

class Container:
    __contains__: Descriptor = Descriptor()

reveal_type(1 in Container())  # revealed: bool
reveal_type("hello" not in Container())  # revealed: bool
```

## Wrong Return Type

Python coerces the results of containment checks to `bool`, even if `__contains__` returns a
non-bool:

```py
class A:
    def __contains__(self, item: str) -> str:
        return "foo"

reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
```

## Literal Result for `in` and `not in`

`__contains__` with a literal return type may result in a `BooleanLiteral` outcome.

```py
from typing import Literal

class AlwaysTrue:
    def __contains__(self, item: int) -> Literal[1]:
        return 1

class AlwaysFalse:
    def __contains__(self, item: int) -> Literal[""]:
        return ""

reveal_type(42 in AlwaysTrue())  # revealed: Literal[True]
reveal_type(42 not in AlwaysTrue())  # revealed: Literal[False]

reveal_type(42 in AlwaysFalse())  # revealed: Literal[False]
reveal_type(42 not in AlwaysFalse())  # revealed: Literal[True]
```

## Required and optional `TypedDict` keys

A required key is always present, while an optional key may or may not be present.

```py
from typing_extensions import NotRequired, TypedDict

class Items(TypedDict):
    required: int
    optional: NotRequired[int]

def membership(items: Items) -> None:
    reveal_type("required" in items)  # revealed: Literal[True]
    reveal_type("required" not in items)  # revealed: Literal[False]
    reveal_type("optional" in items)  # revealed: bool
    reveal_type("optional" not in items)  # revealed: bool
```

## Absent keys in closed `TypedDict`s

A closed `TypedDict` cannot contain an undeclared key or an optional key whose value type is
uninhabited. Declaring `extra_items=Never` closes a `TypedDict` in the same way as `closed=True`.

```py
from typing_extensions import Never, NotRequired, TypedDict

class Closed(TypedDict, closed=True):
    present: int
    impossible: NotRequired[Never]

class ClosedByExtraItems(TypedDict, extra_items=Never):
    present: int

def closed_membership(closed: Closed, closed_by_extra_items: ClosedByExtraItems) -> None:
    reveal_type("missing" in closed)  # revealed: Literal[False]
    reveal_type("missing" not in closed)  # revealed: Literal[True]
    reveal_type("impossible" in closed)  # revealed: Literal[False]
    reveal_type("impossible" not in closed)  # revealed: Literal[True]
    reveal_type("missing" in closed_by_extra_items)  # revealed: Literal[False]
    reveal_type("missing" not in closed_by_extra_items)  # revealed: Literal[True]
```

## Undeclared keys in open `TypedDict`s

Open `TypedDict`s and `TypedDict`s with nonempty extra items may contain keys that their schemas do
not declare.

```py
from typing_extensions import TypedDict

class Open(TypedDict):
    present: int

class ExtraItems(TypedDict, extra_items=int):
    present: int

def open_membership(open_items: Open, extra_items: ExtraItems) -> None:
    reveal_type("missing" in open_items)  # revealed: bool
    reveal_type("missing" not in open_items)  # revealed: bool
    reveal_type("missing" in extra_items)  # revealed: bool
    reveal_type("missing" not in extra_items)  # revealed: bool
```

## `TypedDict` membership with unions and non-literal keys

Membership remains ambiguous when either the key or the `TypedDict` can vary between a present and
an absent alternative. A key missing from every closed alternative is always absent.

```py
from typing_extensions import Literal, TypedDict

class Left(TypedDict, closed=True):
    left: int

class Right(TypedDict, closed=True):
    right: int

def union_membership(
    left: Left,
    either: Left | Right,
    literal_key: Literal["left", "missing"],
    unknown_key: str,
) -> None:
    reveal_type("missing" in either)  # revealed: Literal[False]
    reveal_type("missing" not in either)  # revealed: Literal[True]
    reveal_type("left" in either)  # revealed: bool
    reveal_type(literal_key in left)  # revealed: bool
    reveal_type(unknown_key in left)  # revealed: bool
```

## Functional closed `TypedDict` membership

Functional `TypedDict` definitions expose the same key-presence information as class-based
definitions.

```py
from typing_extensions import TypedDict

Closed = TypedDict("Closed", {"present": int}, closed=True)

def functional_membership(closed: Closed) -> None:
    reveal_type("present" in closed)  # revealed: Literal[True]
    reveal_type("missing" in closed)  # revealed: Literal[False]
```

## No Fallback for `__contains__`

If `__contains__` is implemented, checking membership of a type it doesn't accept is an error; it
doesn't result in a fallback to `__iter__` or `__getitem__`:

```py
class CheckContains: ...
class CheckIter: ...
class CheckGetItem: ...

class CheckIterIterator:
    def __next__(self) -> CheckIter:
        return CheckIter()

class A:
    def __contains__(self, item: CheckContains) -> bool:
        return True

    def __iter__(self) -> CheckIterIterator:
        return CheckIterIterator()

    def __getitem__(self, key: int) -> CheckGetItem:
        return CheckGetItem()

reveal_type(CheckContains() in A())  # revealed: bool

# error: [unsupported-operator] "Operator `in` is not supported between objects of type `CheckIter` and `A`"
reveal_type(CheckIter() in A())  # revealed: bool
# error: [unsupported-operator] "Operator `in` is not supported between objects of type `CheckGetItem` and `A`"
reveal_type(CheckGetItem() in A())  # revealed: bool

class B:
    def __iter__(self) -> CheckIterIterator:
        return CheckIterIterator()

    def __getitem__(self, key: int) -> CheckGetItem:
        return CheckGetItem()

reveal_type(CheckIter() in B())  # revealed: bool
# Always use `__iter__`, regardless of iterated type; there's no NotImplemented
# in this case, so there's no fallback to `__getitem__`
reveal_type(CheckGetItem() in B())  # revealed: bool
```

## Invalid Old-Style Iteration

If `__getitem__` is implemented but does not accept integer arguments, then the membership test is
not supported and should trigger a diagnostic.

```py
class A:
    def __getitem__(self, key: str) -> str:
        return "foo"

# error: [unsupported-operator] "Operator `in` is not supported between objects of type `Literal[42]` and `A`"
reveal_type(42 in A())  # revealed: bool
# error: [unsupported-operator] "Operator `in` is not supported between objects of type `Literal["hello"]` and `A`"
reveal_type("hello" in A())  # revealed: bool
```

## Return type that doesn't implement `__bool__` correctly

`in` and `not in` operations will fail at runtime if the object on the right-hand side of the
operation has a `__contains__` method that returns a type which is not convertible to `bool`. This
is because of the way these operations are handled by the Python interpreter at runtime. If we
assume that `y` is an object that has a `__contains__` method, the Python expression `x in y`
desugars to a `contains(y, x)` call, where `contains` looks something like this:

```ignore
def contains(y, x):
    return bool(type(y).__contains__(y, x))
```

where the `bool()` conversion itself implicitly calls `__bool__` under the hood.

```py
class NotBoolable:
    __bool__: int = 3

class WithContains:
    def __contains__(self, item) -> NotBoolable:
        return NotBoolable()

# snapshot: unsupported-bool-conversion
10 in WithContains()
```

TODO: Ideally the message would explain to the user what's wrong. E.g,

```ignore
error: [operator] cannot use `in` operator on object of type `WithContains`
    note: This is because the `in` operator implicitly calls `WithContains.__contains__`, but `WithContains.__contains__` is invalidly defined
    note: `WithContains.__contains__` is invalidly defined because it returns an instance of `NotBoolable`, which cannot be evaluated in a boolean context
    note: `NotBoolable` cannot be evaluated in a boolean context because its `__bool__` attribute is not callable
```

It may also be more appropriate to use `unsupported-operator` as the error code.

```snapshot
error[unsupported-bool-conversion]: Boolean conversion is not supported for type `NotBoolable`
 --> src/mdtest_snippet.py:9:1
  |
9 | 10 in WithContains()
  | ^^^^^^^^^^^^^^^^^^^^
info: `__bool__` on `NotBoolable` must be callable
```

```py
# snapshot: unsupported-bool-conversion
10 not in WithContains()
```

```snapshot
error[unsupported-bool-conversion]: Boolean conversion is not supported for type `NotBoolable`
  --> src/mdtest_snippet.py:11:1
   |
11 | 10 not in WithContains()
   | ^^^^^^^^^^^^^^^^^^^^^^^^
info: `__bool__` on `NotBoolable` must be callable
```
