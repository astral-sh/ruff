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
# TODO: should emit diagnostic, need to check arg type, will fail
reveal_type(42 in A())  # revealed: bool
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

# TODO: should emit diagnostic, need to check arg type,
# should not fall back to __iter__ or __getitem__
reveal_type(CheckIter() in A())  # revealed: bool
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

# TODO should emit a diagnostic
reveal_type(42 in A())  # revealed: bool
reveal_type("hello" in A())  # revealed: bool
```
