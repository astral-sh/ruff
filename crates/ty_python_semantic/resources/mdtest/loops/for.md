# For loops

## Basic `for` loop

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

for x in IntIterable():
    pass

# revealed: int
# error: [possibly-unresolved-reference]
reveal_type(x)
```

## With previous definition

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

x = "foo"

for x in IntIterable():
    pass

reveal_type(x)  # revealed: Literal["foo"] | int
```

## With `else` (no break)

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

for x in IntIterable():
    pass
else:
    x = "foo"

reveal_type(x)  # revealed: Literal["foo"]
```

## May `break`

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

for x in IntIterable():
    if x > 5:
        break
else:
    x = "foo"

reveal_type(x)  # revealed: int | Literal["foo"]
```

## With old-style iteration protocol

```py
class OldStyleIterable:
    def __getitem__(self, key: int) -> int:
        return 42

for x in OldStyleIterable():
    pass

# revealed: int
# error: [possibly-unresolved-reference]
reveal_type(x)
```

## With heterogeneous tuple

```py
for x in (1, "a", b"foo"):
    pass

# revealed: Literal[1, "a", b"foo"]
# error: [possibly-unresolved-reference]
reveal_type(x)
```

## With non-callable iterator

```py
def _(flag: bool):
    class NotIterable:
        if flag:
            __iter__: int = 1
        else:
            __iter__: None = None

    # snapshot: not-iterable
    for x in NotIterable():
        pass
```

```snapshot
error[not-iterable]: Object of type `NotIterable` is not iterable
 --> src/mdtest_snippet.py:9:14
  |
9 |     for x in NotIterable():
  |              ^^^^^^^^^^^^^
  |
info: Its `__iter__` attribute has type `int | None`, which is not callable
```

```py
    # revealed: Unknown
    # snapshot: possibly-unresolved-reference
    reveal_type(x)
```

```snapshot
info[possibly-unresolved-reference]: Name `x` used when possibly not defined
  --> src/mdtest_snippet.py:13:17
   |
13 |     reveal_type(x)
   |                 ^
   |
```

## Invalid iterable

```py
nonsense = 123
for x in nonsense:  # snapshot: not-iterable
    pass
```

```snapshot
error[not-iterable]: Object of type `Literal[123]` is not iterable
 --> src/mdtest_snippet.py:2:10
  |
2 | for x in nonsense:  # snapshot: not-iterable
  |          ^^^^^^^^
  |
info: It doesn't have an `__iter__` method or a `__getitem__` method
```

## New over old style iteration protocol

```py
class NotIterable:
    def __getitem__(self, key: int) -> int:
        return 42
    __iter__: None = None

for x in NotIterable():  # snapshot: not-iterable
    pass
```

```snapshot
error[not-iterable]: Object of type `NotIterable` is not iterable
 --> src/mdtest_snippet.py:6:10
  |
6 | for x in NotIterable():  # snapshot: not-iterable
  |          ^^^^^^^^^^^^^
  |
info: Its `__iter__` attribute has type `None`, which is not callable
```

## Union type as iterable

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

class Test2:
    def __iter__(self) -> TestIter:
        return TestIter()

def _(flag: bool):
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: int
```

## Union type as iterator

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class TestIter2:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter | TestIter2:
        return TestIter()

for x in Test():
    reveal_type(x)  # revealed: int
```

## Union type as iterable and union type as iterator

```py
class Result1A: ...
class Result1B: ...
class Result2A: ...
class Result2B: ...
class Result3: ...
class Result4: ...

class TestIter1:
    def __next__(self) -> Result1A | Result1B:
        return Result1B()

class TestIter2:
    def __next__(self) -> Result2A | Result2B:
        return Result2B()

class TestIter3:
    def __next__(self) -> Result3:
        return Result3()

class TestIter4:
    def __next__(self) -> Result4:
        return Result4()

class Test:
    def __iter__(self) -> TestIter1 | TestIter2:
        return TestIter1()

class Test2:
    def __iter__(self) -> TestIter3 | TestIter4:
        return TestIter3()

def _(flag: bool):
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: Result1A | Result1B | Result2A | Result2B | Result3 | Result4
```

## Union type as iterable where `Iterator[]` is used as the return type of `__iter__`

This test differs from the above tests in that `Iterator` (an abstract type) is used as the return
annotation of the `__iter__` methods, rather than a concrete type being used as the return
annotation.

```py
from typing import Iterator, Literal

class IntIterator:
    def __iter__(self) -> Iterator[int]:
        return iter(range(42))

class StrIterator:
    def __iter__(self) -> Iterator[str]:
        return iter("foo")

def f(x: IntIterator | StrIterator):
    for a in x:
        reveal_type(a)  # revealed: int | str
```

Most real-world iterable types use `Iterator` as the return annotation of their `__iter__` methods:

```py
def g(
    a: tuple[int, ...] | tuple[str, ...],
    b: list[str] | list[int],
    c: Literal["foo", b"bar"],
):
    for x in a:
        reveal_type(x)  # revealed: int | str
    for y in b:
        reveal_type(y)  # revealed: str | int
```

## Union type as iterable where some elements in the union have precise tuple specs

If all elements in a union can be iterated over, we "union together" their "tuple specs" and are
able to infer the iterable element precisely when iterating over the union, in the same way that we
infer a precise type for the iterable element when iterating over a `Literal` string or bytes type:

```py
from typing import Literal

def f(x: Literal["foo", b"bar"], y: Literal["foo"] | range):
    for item in x:
        reveal_type(item)  # revealed: Literal["f", "o", 98, 97, 114]
    for item in y:
        reveal_type(item)  # revealed: Literal["f", "o"] | int
```

## Attribute errors from iterated aliased unions

We should still report missing attributes when a loop variable comes from an aliased union element:

```toml
[environment]
python-version = "3.12"
```

```py
class A:
    pass

class B:
    def do_b_thing(self) -> None:
        pass

type U = A | B

class C:
    def __init__(self, values: list[U]) -> None:
        self.values = values

    def f(self) -> None:
        for item in self.values:
            reveal_type(item)  # revealed: A | B
            # error: [unresolved-attribute] "Attribute `do_b_thing` is not defined on `A` in union `U`"
            item.do_b_thing()
```

## Union type as iterable where one union element has no `__iter__` method

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

def _(flag: bool):
    # snapshot: not-iterable
    for x in Test() if flag else 42:
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Test | Literal[42]` may not be iterable
  --> src/mdtest_snippet.py:11:14
   |
11 |     for x in Test() if flag else 42:
   |              ^^^^^^^^^^^^^^^^^^^^^^
   |
info: It may not have an `__iter__` method and it doesn't have a `__getitem__` method
info: `Literal[42]` does not implement `__iter__`
```

## Union type as iterable where one union element has invalid `__iter__` method

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

class Test2:
    def __iter__(self) -> int:
        return 42

def _(flag: bool):
    # TODO: Improve error message to state which union variant isn't iterable (https://github.com/astral-sh/ruff/issues/13989)
    # snapshot: not-iterable
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Test | Test2` may not be iterable
  --> src/mdtest_snippet.py:16:14
   |
16 |     for x in Test() if flag else Test2():
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `TestIter | int`, which may not have a `__next__` method
```

## Union type as iterable where one union element has a non-callable `__iter__`

When one union element has a callable `__iter__` and another has a non-callable `__iter__`
attribute, the error should be "may not be iterable" (hedged), not "is not iterable" (definitive) —
because at runtime the value might be the iterable variant.

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

class NotIter:
    # `__iter__` is present but not callable
    __iter__: int = 32

def _(flag: bool):
    iterable = Test() if flag else NotIter()
    # snapshot: not-iterable
    for x in iterable:
        reveal_type(x)  # revealed: int | Unknown
```

```snapshot
error[not-iterable]: Object of type `Test | NotIter` may not be iterable
  --> src/mdtest_snippet.py:16:14
   |
16 |     for x in iterable:
   |              ^^^^^^^^
   |
info: Its `__iter__` attribute (with type `(bound method Test.__iter__() -> TestIter) | int`) may not be callable
```

## Union type as iterator where one union element has no `__next__` method

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter | int:
        return TestIter()

# error: [not-iterable] "Object of type `Test` may not be iterable"
for x in Test():
    reveal_type(x)  # revealed: int
```

## Intersection type via isinstance narrowing

When we have an intersection type via `isinstance` narrowing, we should be able to infer the
iterable element type precisely:

```py
from typing import Sequence

def _(x: Sequence[int], y: object):
    reveal_type(x)  # revealed: Sequence[int]
    for item in x:
        reveal_type(item)  # revealed: int

    if isinstance(y, list):
        reveal_type(y)  # revealed: Top[list[Unknown]]
        for item in y:
            reveal_type(item)  # revealed: object

    if isinstance(x, list):
        reveal_type(x)  # revealed: Sequence[int] & Top[list[Unknown]]
        for item in x:
            # int & object simplifies to int
            reveal_type(item)  # revealed: int
```

## Intersection where some elements are not iterable

When iterating over an intersection type, we should only fail if all positive elements fail to
iterate. If some elements are iterable and some are not, we should iterate over the iterable ones
and intersect their element types.

```py
from ty_extensions import Intersection

class NotIterable:
    pass

def _(x: Intersection[list[int], NotIterable]):
    # `list[int]` is iterable (yielding `int`), but `NotIterable` is not.
    # We should still be able to iterate over the intersection.
    for item in x:
        reveal_type(item)  # revealed: int
```

## Intersection where all elements are not iterable

When iterating over an intersection type where all positive elements are not iterable, we should
fail to iterate.

```py
from ty_extensions import Intersection

class NotIterable1:
    pass

class NotIterable2:
    pass

def _(x: Intersection[NotIterable1, NotIterable2]):
    # error: [not-iterable]
    for item in x:
        reveal_type(item)  # revealed: Unknown
```

## Intersection of fixed-length tuples

When iterating over an intersection of two fixed-length tuples with the same length, we should
intersect the element types position-by-position.

```py
from ty_extensions import Intersection

def _(x: Intersection[tuple[int, str], tuple[object, object]]):
    # `tuple[int, str]` yields `int | str` when iterated.
    # `tuple[object, object]` yields `object` when iterated.
    # The intersection should yield `(int & object) | (str & object)` = `int | str`.
    for item in x:
        reveal_type(item)  # revealed: int | str
```

## Intersection of variable-length and fixed-length tuple

When iterating over an intersection of a variable-length tuple with a fixed-length tuple, we should
preserve the fixed-length structure and intersect each element type with the variable-length tuple's
element type.

```py
from ty_extensions import Intersection

def _(x: Intersection[tuple[str, ...], tuple[object, object]]):
    # `tuple[str, ...]` yields `str` when iterated.
    # `tuple[object, object]` yields `object` when iterated.
    # The intersection should yield `(str & object) | (str & object)` = `str`.
    for item in x:
        reveal_type(item)  # revealed: str
```

## Intersection of variable-length tuples

When iterating over an intersection of two variable-length tuples, we should intersect the element
types position-by-position.

```toml
[environment]
python-version = "3.11"
```

```py
from ty_extensions import Intersection

def _(x: Intersection[tuple[int, *tuple[str, ...], bytes], tuple[object, *tuple[str, ...]]]):
    # After resizing, the intersection becomes:
    # tuple[int & object, *tuple[str & str, ...], bytes & str]
    # = tuple[int, *tuple[str, ...], Never]
    # Iterating yields: int | str | Never = int | str
    for item in x:
        reveal_type(item)  # revealed: int | str
```

## Intersection of fixed-length tuple with homogeneous iterable

When iterating over an intersection of a fixed-length tuple with a class that implements `__iter__`
returning a homogeneous iterator, we should preserve the fixed-length structure and intersect each
element type with the iterator's element type.

```py
from collections.abc import Iterator

class Foo:
    def __iter__(self) -> Iterator[object]:
        raise NotImplementedError

def _(x: tuple[int, str, bytes]):
    if isinstance(x, Foo):
        # The intersection `tuple[int, str, bytes] & Foo` should iterate as
        # `tuple[int & object, str & object, bytes & object]` = `tuple[int, str, bytes]`
        a, b, c = x
        reveal_type(a)  # revealed: int
        reveal_type(b)  # revealed: str
        reveal_type(c)  # revealed: bytes
        reveal_type(tuple(x))  # revealed: tuple[int, str, bytes]
```

## Intersection of homogeneous iterables

When iterating over an intersection of two types that both yield homogeneous variable-length tuple
specs, we should intersect their element types.

```py
from collections.abc import Iterator

class Foo:
    def __iter__(self) -> Iterator[object]:
        raise NotImplementedError

def _(x: list[int]):
    if isinstance(x, Foo):
        # `list[int]` yields `int`, `Foo` yields `object`.
        # The intersection should yield `int & object` = `int`.
        for item in x:
            reveal_type(item)  # revealed: int
```

## Possibly-not-callable `__iter__` method

```py
def _(flag: bool):
    class Iterator:
        def __next__(self) -> int:
            return 42

    class CustomCallable:
        if flag:
            def __call__(self, *args, **kwargs) -> Iterator:
                return Iterator()

        else:
            __call__: None = None

    class Iterable1:
        __iter__: CustomCallable = CustomCallable()

    class Iterable2:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()

        else:
            __iter__: None = None

    # error: [not-iterable] "Object of type `Iterable1` may not be iterable"
    for x in Iterable1():
        # TODO... `int` might be ideal here?
        reveal_type(x)  # revealed: int | Unknown

    # error: [not-iterable] "Object of type `Iterable2` may not be iterable"
    for y in Iterable2():
        # TODO... `int` might be ideal here?
        reveal_type(y)  # revealed: int | Unknown
```

## `__iter__` method with a bad signature

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self, extra_arg) -> Iterator:
        return Iterator()

# snapshot: not-iterable
for x in Iterable():
    reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Iterable` is not iterable
  --> src/mdtest_snippet.py:10:10
   |
10 | for x in Iterable():
   |          ^^^^^^^^^^
   |
info: Its `__iter__` method has an invalid signature
info: Expected signature `def __iter__(self): ...`
```

## `__iter__` does not return an iterator

```py
class Bad:
    def __iter__(self) -> int:
        return 42

# snapshot: not-iterable
for x in Bad():
    reveal_type(x)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `Bad` is not iterable
 --> src/mdtest_snippet.py:6:10
  |
6 | for x in Bad():
  |          ^^^^^
  |
info: Its `__iter__` method returns an object of type `int`, which has no `__next__` method
```

## `__iter__` returns an object with a possibly missing `__next__` method

```py
def _(flag: bool):
    class Iterator:
        if flag:
            def __next__(self) -> int:
                return 42

    class Iterable:
        def __iter__(self) -> Iterator:
            return Iterator()

    # error: [not-iterable] "Object of type `Iterable` may not be iterable"
    for x in Iterable():
        reveal_type(x)  # revealed: int
```

## `__iter__` returns an iterator with an invalid `__next__` method

```py
class Iterator1:
    def __next__(self, extra_arg) -> int:
        return 42

class Iterator2:
    __next__: None = None

class Iterable1:
    def __iter__(self) -> Iterator1:
        return Iterator1()

class Iterable2:
    def __iter__(self) -> Iterator2:
        return Iterator2()

# snapshot: not-iterable
for x in Iterable1():
    reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Iterable1` is not iterable
  --> src/mdtest_snippet.py:17:10
   |
17 | for x in Iterable1():
   |          ^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `Iterator1`, which has an invalid `__next__` method
info: Expected signature for `__next__` is `def __next__(self): ...`
```

```py
# snapshot: not-iterable
for y in Iterable2():
    reveal_type(y)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable2` is not iterable
  --> src/mdtest_snippet.py:20:10
   |
20 | for y in Iterable2():
   |          ^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `Iterator2`, which has a `__next__` attribute that is not callable
```

## Possibly missing `__iter__` and bad `__getitem__` method

```py
def _(flag: bool):
    class Iterator:
        def __next__(self) -> int:
            return 42

    class Iterable:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()
        # invalid signature because it only accepts a `str`,
        # but the old-style iteration protocol will pass it an `int`
        def __getitem__(self, key: str) -> bytes:
            return bytes()

    # snapshot: not-iterable
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
```

```snapshot
error[not-iterable]: Object of type `Iterable` may not be iterable
  --> src/mdtest_snippet.py:16:14
   |
16 |     for x in Iterable():
   |              ^^^^^^^^^^
   |
info: It may not have an `__iter__` method and its `__getitem__` method has an incorrect signature for the old-style iteration protocol
info: `__getitem__` must be at least as permissive as `def __getitem__(self, key: int): ...` to satisfy the old-style iteration protocol
```

## Possibly missing `__iter__` and not-callable `__getitem__`

This snippet tests that we infer the element type correctly in the following edge case:

- `__iter__` is a method with the correct parameter spec that returns a valid iterator; BUT
- `__iter__` is possibly missing; AND
- `__getitem__` is set to a non-callable type

It's important that we emit a diagnostic here, but it's also important that we still use the return
type of the iterator's `__next__` method as the inferred type of `x` in the `for` loop:

```py
def _(flag: bool):
    class Iterator:
        def __next__(self) -> int:
            return 42

    class Iterable:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()
        __getitem__: None = None

    # error: [not-iterable] "Object of type `Iterable` may not be iterable"
    for x in Iterable():
        reveal_type(x)  # revealed: int
```

## Possibly missing `__iter__` and possibly missing `__getitem__`

```py
class Iterator:
    def __next__(self) -> int:
        return 42

def _(flag1: bool, flag2: bool):
    class Iterable:
        if flag1:
            def __iter__(self) -> Iterator:
                return Iterator()
        if flag2:
            def __getitem__(self, key: int) -> bytes:
                return bytes()

    # snapshot: not-iterable
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
```

```snapshot
error[not-iterable]: Object of type `Iterable` may not be iterable
  --> src/mdtest_snippet.py:15:14
   |
15 |     for x in Iterable():
   |              ^^^^^^^^^^
   |
info: It may not have an `__iter__` method or a `__getitem__` method
```

## No `__iter__` method and `__getitem__` is not callable

```py
class Bad:
    __getitem__: None = None

# snapshot: not-iterable
for x in Bad():
    reveal_type(x)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `Bad` is not iterable
 --> src/mdtest_snippet.py:5:10
  |
5 | for x in Bad():
  |          ^^^^^
  |
info: It has no `__iter__` method and its `__getitem__` attribute has type `None`, which is not callable
```

## Possibly-not-callable `__getitem__` method

```py
def _(flag: bool):
    class CustomCallable:
        if flag:
            def __call__(self, *args, **kwargs) -> int:
                return 42

        else:
            __call__: None = None

    class Iterable1:
        __getitem__: CustomCallable = CustomCallable()

    class Iterable2:
        if flag:
            def __getitem__(self, key: int) -> int:
                return 42

        else:
            __getitem__: None = None

    # snapshot: not-iterable
    for x in Iterable1():
        # TODO... `int` might be ideal here?
        reveal_type(x)  # revealed: int | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable1` may not be iterable
  --> src/mdtest_snippet.py:22:14
   |
22 |     for x in Iterable1():
   |              ^^^^^^^^^^^
   |
info: It has no `__iter__` method and its `__getitem__` attribute is invalid
info: `__getitem__` has type `CustomCallable`, which is not callable
```

```py
    # snapshot: not-iterable
    for y in Iterable2():
        # TODO... `int` might be ideal here?
        reveal_type(y)  # revealed: int | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable2` may not be iterable
  --> src/mdtest_snippet.py:26:14
   |
26 |     for y in Iterable2():
   |              ^^^^^^^^^^^
   |
info: It has no `__iter__` method and its `__getitem__` attribute is invalid
info: `__getitem__` has type `(bound method Iterable2.__getitem__(key: int) -> int) | None`, which is not callable
```

## Bad `__getitem__` method

```py
class Iterable:
    # invalid because it will implicitly be passed an `int`
    # by the interpreter
    def __getitem__(self, key: str) -> int:
        return 42

# snapshot: not-iterable
for x in Iterable():
    reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Iterable` is not iterable
 --> src/mdtest_snippet.py:8:10
  |
8 | for x in Iterable():
  |          ^^^^^^^^^^
  |
info: It has no `__iter__` method and its `__getitem__` method has an incorrect signature for the old-style iteration protocol
info: `__getitem__` must be at least as permissive as `def __getitem__(self, key: int): ...` to satisfy the old-style iteration protocol
```

## Possibly missing `__iter__` but definitely bound `__getitem__`

Here, we should not emit a diagnostic: if `__iter__` is unbound, we should fallback to
`__getitem__`:

```py
class Iterator:
    def __next__(self) -> str:
        return "foo"

def _(flag: bool):
    class Iterable:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()

        def __getitem__(self, key: int) -> bytes:
            return b"foo"

    for x in Iterable():
        reveal_type(x)  # revealed: str | bytes
```

## Possibly invalid `__iter__` methods

```py
class Iterator:
    def __next__(self) -> int:
        return 42

def _(flag: bool):
    class Iterable1:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()

        else:
            def __iter__(self, invalid_extra_arg) -> Iterator:
                return Iterator()

    # snapshot: not-iterable
    for x in Iterable1():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `Iterable1` may not be iterable
  --> src/mdtest_snippet.py:16:14
   |
16 |     for x in Iterable1():
   |              ^^^^^^^^^^^
   |
info: Its `__iter__` method may have an invalid signature
info: Type of `__iter__` is `(bound method Iterable1.__iter__() -> Iterator) | (bound method Iterable1.__iter__(invalid_extra_arg) -> Iterator)`
info: Expected signature for `__iter__` is `def __iter__(self): ...`
```

```py
    class Iterable2:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()

        else:
            __iter__: None = None

    # snapshot: not-iterable
    for x in Iterable2():
        # TODO: `int` would probably be better here:
        reveal_type(x)  # revealed: int | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable2` may not be iterable
  --> src/mdtest_snippet.py:27:14
   |
27 |     for x in Iterable2():
   |              ^^^^^^^^^^^
   |
info: Its `__iter__` attribute (with type `(bound method Iterable2.__iter__() -> Iterator) | None`) may not be callable
```

## Possibly invalid `__next__` method

```py
def _(flag: bool):
    class Iterator1:
        if flag:
            def __next__(self) -> int:
                return 42

        else:
            def __next__(self, invalid_extra_arg) -> str:
                return "foo"

    class Iterator2:
        if flag:
            def __next__(self) -> int:
                return 42

        else:
            __next__: None = None

    class Iterable1:
        def __iter__(self) -> Iterator1:
            return Iterator1()

    class Iterable2:
        def __iter__(self) -> Iterator2:
            return Iterator2()

    # snapshot: not-iterable
    for x in Iterable1():
        reveal_type(x)  # revealed: int | str
```

```snapshot
error[not-iterable]: Object of type `Iterable1` may not be iterable
  --> src/mdtest_snippet.py:28:14
   |
28 |     for x in Iterable1():
   |              ^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `Iterator1`, which may have an invalid `__next__` method
info: Expected signature for `__next__` is `def __next__(self): ...`
```

```py
    # snapshot: not-iterable
    for y in Iterable2():
        # TODO: `int` would probably be better here:
        reveal_type(y)  # revealed: int | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable2` may not be iterable
  --> src/mdtest_snippet.py:31:14
   |
31 |     for y in Iterable2():
   |              ^^^^^^^^^^^
   |
info: Its `__iter__` method returns an object of type `Iterator2`, which has a `__next__` attribute that may not be callable
```

## Possibly invalid `__getitem__` methods

```py
def _(flag: bool):
    class Iterable1:
        if flag:
            def __getitem__(self, item: int) -> str:
                return "foo"

        else:
            __getitem__: None = None

    class Iterable2:
        if flag:
            def __getitem__(self, item: int) -> str:
                return "foo"

        else:
            def __getitem__(self, item: str) -> int:
                return 42

    # snapshot: not-iterable
    for x in Iterable1():
        # TODO: `str` might be better
        reveal_type(x)  # revealed: str | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable1` may not be iterable
  --> src/mdtest_snippet.py:20:14
   |
20 |     for x in Iterable1():
   |              ^^^^^^^^^^^
   |
info: It has no `__iter__` method and its `__getitem__` attribute is invalid
info: `__getitem__` has type `(bound method Iterable1.__getitem__(item: int) -> str) | None`, which is not callable
```

```py
    # snapshot: not-iterable
    for y in Iterable2():
        reveal_type(y)  # revealed: str | int
```

```snapshot
error[not-iterable]: Object of type `Iterable2` may not be iterable
  --> src/mdtest_snippet.py:24:14
   |
24 |     for y in Iterable2():
   |              ^^^^^^^^^^^
   |
info: It has no `__iter__` method and its `__getitem__` method (with type `(bound method Iterable2.__getitem__(item: int) -> str) | (bound method Iterable2.__getitem__(item: str) -> int)`) may have an incorrect signature for the old-style iteration protocol
info: `__getitem__` must be at least as permissive as `def __getitem__(self, key: int): ...` to satisfy the old-style iteration protocol
```

## Possibly missing `__iter__` and possibly invalid `__getitem__`

```py
class Iterator:
    def __next__(self) -> bytes:
        return b"foo"

def _(flag: bool, flag2: bool):
    class Iterable1:
        if flag:
            def __getitem__(self, item: int) -> str:
                return "foo"

        else:
            __getitem__: None = None

        if flag2:
            def __iter__(self) -> Iterator:
                return Iterator()

    class Iterable2:
        if flag:
            def __getitem__(self, item: int) -> str:
                return "foo"

        else:
            def __getitem__(self, item: str) -> int:
                return 42
        if flag2:
            def __iter__(self) -> Iterator:
                return Iterator()

    # snapshot: not-iterable
    for x in Iterable1():
        # TODO: `bytes | str` might be better
        reveal_type(x)  # revealed: bytes | str | Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterable1` may not be iterable
  --> src/mdtest_snippet.py:31:14
   |
31 |     for x in Iterable1():
   |              ^^^^^^^^^^^
   |
info: It may not have an `__iter__` method and its `__getitem__` attribute (with type `(bound method Iterable1.__getitem__(item: int) -> str) | None`) may not be callable
```

```py
    # snapshot: not-iterable
    for y in Iterable2():
        reveal_type(y)  # revealed: bytes | str | int
```

```snapshot
error[not-iterable]: Object of type `Iterable2` may not be iterable
  --> src/mdtest_snippet.py:35:14
   |
35 |     for y in Iterable2():
   |              ^^^^^^^^^^^
   |
info: It may not have an `__iter__` method and its `__getitem__` method (with type `(bound method Iterable2.__getitem__(item: int) -> str) | (bound method Iterable2.__getitem__(item: str) -> int)`) may have an incorrect signature for the old-style iteration protocol
info: `__getitem__` must be at least as permissive as `def __getitem__(self, key: int): ...` to satisfy the old-style iteration protocol
```

## Empty tuple is iterable

```py
for x in ():
    reveal_type(x)  # revealed: Never
```

## Never is iterable

```py
from typing_extensions import Never

def f(never: Never):
    for x in never:
        reveal_type(x)  # revealed: Unknown
```

## Iterating over literals

```py
from typing import Literal

for char in "abcde":
    reveal_type(char)  # revealed: Literal["a", "b", "c", "d", "e"]

for char in b"abcde":
    reveal_type(char)  # revealed: Literal[97, 98, 99, 100, 101]
```

## A class literal is iterable if it inherits from `Any`

A class literal can be iterated over if it has `Any` or `Unknown` in its MRO, since the
`Any`/`Unknown` element in the MRO could materialize to a class with a custom metaclass that defines
`__iter__` for all instances of the metaclass:

```py
from unresolved_module import SomethingUnknown  # error: [unresolved-import]
from typing import Any, Iterable
from ty_extensions import static_assert, is_assignable_to, TypeOf, Unknown, reveal_mro

class Foo(SomethingUnknown): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

# TODO: these should pass
static_assert(is_assignable_to(TypeOf[Foo], Iterable[Unknown]))  # error: [static-assert-error]
static_assert(is_assignable_to(type[Foo], Iterable[Unknown]))  # error: [static-assert-error]

# TODO: should not error
# error: [not-iterable]
for x in Foo:
    reveal_type(x)  # revealed: Unknown

class Bar(Any): ...

reveal_mro(Bar)  # revealed: (<class 'Bar'>, Any, <class 'object'>)

# TODO: these should pass
static_assert(is_assignable_to(TypeOf[Bar], Iterable[Any]))  # error: [static-assert-error]
static_assert(is_assignable_to(type[Bar], Iterable[Any]))  # error: [static-assert-error]

# TODO: should not error
# error: [not-iterable]
for x in Bar:
    # TODO: should reveal `Any`
    reveal_type(x)  # revealed: Unknown
```

## Iterating over an intersection with a TypeVar whose bound is a union

When a TypeVar has a union bound and the TypeVar is intersected with an iterable type (e.g., via
`isinstance`), we need to properly distribute the intersection over the union and simplify. This
ensures that only the parts of the union compatible with the intersection are considered for
iteration.

```toml
[environment]
python-version = "3.12"
```

### TypeVar bound with non-iterable elements

When the union contains non-iterable types (like `int`), those parts are disjoint from the tuple and
simplify to `Never`, leaving only the iterable parts.

```py
def f[T: tuple[int, ...] | int](x: T):
    if isinstance(x, tuple):
        reveal_type(x)  # revealed: T@f & tuple[object, ...]
        for item in x:
            # The intersection `(tuple[int, ...] | int) & tuple[object, ...]` distributes to:
            # `(tuple[int, ...] & tuple[object, ...]) | (int & tuple[object, ...])`
            # which simplifies to `tuple[int, ...] | Never` = `tuple[int, ...]`
            # so iterating gives `int`.
            reveal_type(item)  # revealed: int
```

### TypeVar bound with all iterable but disjoint elements

When the union contains types that are all iterable but some are disjoint from the intersection
constraint, those parts should also simplify to `Never`.

```py
def g[T: tuple[int, ...] | list[str]](x: T):
    if isinstance(x, tuple):
        reveal_type(x)  # revealed: T@g & tuple[object, ...]
        for item in x:
            # The intersection `(tuple[int, ...] | list[str]) & tuple[object, ...]` distributes to:
            # `(tuple[int, ...] & tuple[object, ...]) | (list[str] & tuple[object, ...])`
            # Since `list[str]` is disjoint from `tuple[object, ...]`, this simplifies to:
            # `tuple[int, ...] | Never` = `tuple[int, ...]`
            # so iterating gives `int`, NOT `int | str`.
            reveal_type(item)  # revealed: int
```

## Iterating over a list with a negated type parameter

When we have a list with a negated type parameter (e.g., `list[~str]`), we should still be able to
iterate over it correctly. The negated type parameter represents all types except `str`, and
`list[~str]` is still a valid list that can be iterated.

```py
from ty_extensions import Not

def _(value: list[Not[str]]):
    for x in value:
        reveal_type(x)  # revealed: ~str
```

## Walrus definitions in the iterator expression are always evaluated

```py
for _ in (x := range(0)):
    pass
reveal_type(x)  # revealed: range
```

## Cyclic control flow

### Basic

```py
i = 0
reveal_type(i)  # revealed: Literal[0]
for _ in range(1_000_000):
    i += 1
    reveal_type(i)  # revealed: int
reveal_type(i)  # revealed: int
```

### A binding that didn't exist before the loop started

```py
i = 0
for _ in range(1_000_000):
    if i > 0:
        loop_only += 1  # error: [possibly-unresolved-reference]
    if i == 0:
        loop_only = 0
    i += 1
# error: [possibly-unresolved-reference]
reveal_type(loop_only)  # revealed: int
```

### Nested loops with `break` and `continue`

```py
def random() -> bool:
    return False

x = "A"
for _ in range(1_000_000):
    reveal_type(x)  # revealed: Literal["A", "D"]
    for _ in range(1_000_000):
        # The "C" binding isn't visible here. It breaks this inner loop, and it always gets
        # overwritten before the end of the outer loop.
        reveal_type(x)  # revealed: Literal["A", "D", "B"]
        if random():
            x = "B"
            continue
        else:
            x = "C"
            break
        reveal_type(x)  # revealed: Never
    # We don't know whether a `for` loop will execute its body at all, so "A" is still visible here.
    # Similarly, we don't know when the loop will terminate, so "B" is also visible here despite the
    # `continue` above.
    reveal_type(x)  # revealed: Literal["A", "D", "B", "C"]
    if random():
        x = "D"
        continue
    else:
        x = "E"
        break
    reveal_type(x)  # revealed: Never
reveal_type(x)  # revealed: Literal["A", "D", "E"]
```

### Walrus operator assignments are visible via loopback

```py
for _ in range(1_000_000):
    # error: [possibly-unresolved-reference]
    reveal_type(y)  # revealed: Literal[1]
    x = (y := 1)
```

### Loopback bindings are not visible to the walrus operator in iterable expression

The iterable is only evaluated once, before the loop body runs.

```py
x = "hello"
for _ in (y := x):
    # This assignment is not visible when the iterable `x` is used above.
    x = None
reveal_type(y)  # revealed: Literal["hello"]
```

### "Member" (as opposed to "symbol") places are also given loopback bindings

```py
my_dict = {}
my_dict["x"] = 0
reveal_type(my_dict["x"])  # revealed: Literal[0]
for _ in range(1_000_000):
    my_dict["x"] += 1
reveal_type(my_dict["x"])  # revealed: int
```

### `del` prevents bindings from reaching the loopback

This `x` cannot reach the use at the top of the loop:

```py
for _ in range(1_000_000):
    x  # error: [unresolved-reference]
    x = 42
    del x
```

On the other hand, if `x` is defined before the loop, the `del` makes it a
`[possibly-unresolved-reference]`:

```py
x = 0
for _ in range(1_000_000):
    x  # error: [possibly-unresolved-reference]
    x = 42
    del x
```

### `del` in a loop makes a variable possibly-unbound after the loop

```py
x = 0
for _ in range(1_000_000):
    # error: [possibly-unresolved-reference]
    del x
# error: [possibly-unresolved-reference]
x
```

### Bindings in a loop are possibly-unbound after the loop

```py
for _ in range(1_000_000):
    x = 42
# error: [possibly-unresolved-reference]
x
```

### Swap bindings converge normally under fixpoint iteration

```py
x = 1
y = 2
for _ in range(1_000_000):
    x, y = y, x
    reveal_type(x)  # revealed: Literal[2, 1]
    reveal_type(y)  # revealed: Literal[1, 2]
```

### Tuple assignments are inferred correctly

```py
x = 0
for _ in range(1_000_000):
    x, y = x + 1, None
    reveal_type(x)  # revealed: int
```

### Avoid oscillations

We need to avoid oscillating cycles in cases like the following, where the type of one of these loop
variables also influences the static reachability of its bindings. This case was minimized from a
real crash that came up during development checking these lines of `sympy`:
<https://github.com/sympy/sympy/blob/c2bfd65accf956576b58f0ae57bf5821a0c4ff49/sympy/core/numbers.py#L158-L166>

```py
x = 1
y = 2
for _ in range(1_000_000):
    if x:
        x, y = y, x
    reveal_type(x)  # revealed: Literal[2, 1]
    reveal_type(y)  # revealed: Literal[1, 2]
```

### Bindings in statically unreachable branches are excluded from loopback

```py
VAL = 1

x = 1
for _ in range(1_000_000):
    reveal_type(x)  # revealed: Literal[1]
    if VAL - 1:
        x = 2
```

### Large reachability constraint graphs fall back to `Unknown`

```py
def f(items, flags):
    x = 1
    for item in items:
        # This example is just over the exact loop-header reachability cutoff. If it falls
        # below the cutoff, this line reveals `Literal[1, 2]`.
        reveal_type(x)  # revealed: Literal[1] | Unknown
        if flags[200]:
            x = 2
    for item0 in items:
        if flags[0]:
            x = item0
        for item1 in items:
            if flags[1]:
                x = item1
            for item2 in items:
                if flags[2]:
                    x = item2
                for item3 in items:
                    if flags[3]:
                        x = item3
                    for item4 in items:
                        if flags[4]:
                            x = item4
                        for item5 in items:
                            if flags[5]:
                                x = item5
        for item6 in items:
            if flags[6]:
                x = item6
            for item7 in items:
                if flags[7]:
                    x = item7
                for item8 in items:
                    if flags[8]:
                        x = item8
                    for item9 in items:
                        if flags[9]:
                            x = item9
                        for item10 in items:
                            if flags[10]:
                                x = item10
    for item11 in items:
        if flags[11]:
            x = item11
        for item12 in items:
            if flags[12]:
                x = item12
            for item13 in items:
                if flags[13]:
                    x = item13
                for item14 in items:
                    if flags[14]:
                        x = item14
                    for item15 in items:
                        if flags[15]:
                            x = item15
                        for item16 in items:
                            if flags[16]:
                                x = item16
        for item17 in items:
            if flags[17]:
                x = item17
            for item18 in items:
                if flags[18]:
                    x = item18
                for item19 in items:
                    if flags[19]:
                        x = item19
                    for item20 in items:
                        if flags[20]:
                            x = item20
                        for item21 in items:
                            if flags[21]:
                                x = item21
    if flags[100]:
        x = 0
    if flags[101]:
        x = 1
    if flags[102]:
        x = 2
    if flags[103]:
        x = 3
    if flags[104]:
        x = 4
    if flags[105]:
        x = 5
    if flags[106]:
        x = 6
    if flags[107]:
        x = 7
    if flags[108]:
        x = 8
    if flags[109]:
        x = 9
    if flags[110]:
        x = 10
    if flags[111]:
        x = 11
    if flags[112]:
        x = 12
    if flags[113]:
        x = 13
    if flags[114]:
        x = 14
    if flags[115]:
        x = 15
    if flags[116]:
        x = 16
    if flags[117]:
        x = 17
    if flags[118]:
        x = 18
    if flags[119]:
        x = 19
    if flags[120]:
        x = 20
    if flags[121]:
        x = 21
    if flags[122]:
        x = 22
    if flags[123]:
        x = 23
    if flags[124]:
        x = 24
    if flags[125]:
        x = 25
    if flags[126]:
        x = 26
```

### `Divergent` in narrowing conditions doesn't run afoul of "monotonic widening" in cycle recovery

This test looks for a complicated inference failure case that came up during implementation. See the
`while` variant of this case in `while_loop.md` for a detailed description.

```py
class Node:
    def __init__(self, next: "Node | None" = None):
        self.next: "Node | None" = next

node = Node(Node(Node()))
for _ in range(1_000_000):
    if node.next is None:
        break
    node = node.next
reveal_type(node)  # revealed: Node
reveal_type(node.next)  # revealed: Node | None
```

### `global` and `nonlocal` keywords in a loop

We need to make sure that the loop header definition doesn't count as a "use" prior to the
`global`/`nonlocal` declaration, or else we'll emit a false-positive semantic syntax error.

```py
x = 0

def _():
    y = 0
    def _():
        for _ in range(1_000_000):
            global x
            nonlocal y
            x = 42
            y = 99
```

On the other hand, we don't want to shadow true positives:

```py
x = 0

def _():
    y = 0
    def _():
        x = 1
        y = 1
        for _ in range(1_000_000):
            global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
            nonlocal y  # error: [invalid-syntax] "name `y` is used prior to nonlocal declaration"
```

### Loop header definitions don't shadow member bindings

```py
class C:
    x = None

c = C()
c.x = 0

for _ in range(1):
    reveal_type(c.x)  # revealed: Literal[0]
    c = C()
    break

d = [0]
d[0] = 1

for _ in range(1):
    reveal_type(d[0])  # revealed: Literal[1]
    d = []
    break
```
