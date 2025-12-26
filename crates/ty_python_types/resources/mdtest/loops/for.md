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

<!-- snapshot-diagnostics -->

```py
def _(flag: bool):
    class NotIterable:
        if flag:
            __iter__: int = 1
        else:
            __iter__: None = None

    # error: [not-iterable]
    for x in NotIterable():
        pass

    # revealed: Unknown
    # error: [possibly-unresolved-reference]
    reveal_type(x)
```

## Invalid iterable

<!-- snapshot-diagnostics -->

```py
nonsense = 123
for x in nonsense:  # error: [not-iterable]
    pass
```

## New over old style iteration protocol

<!-- snapshot-diagnostics -->

```py
class NotIterable:
    def __getitem__(self, key: int) -> int:
        return 42
    __iter__: None = None

for x in NotIterable():  # error: [not-iterable]
    pass
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

## Union type as iterable where one union element has no `__iter__` method

<!-- snapshot-diagnostics -->

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

def _(flag: bool):
    # error: [not-iterable]
    for x in Test() if flag else 42:
        reveal_type(x)  # revealed: int
```

## Union type as iterable where one union element has invalid `__iter__` method

<!-- snapshot-diagnostics -->

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
    # error: [not-iterable]
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: int
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

<!-- snapshot-diagnostics -->

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self, extra_arg) -> Iterator:
        return Iterator()

# error: [not-iterable]
for x in Iterable():
    reveal_type(x)  # revealed: int
```

## `__iter__` does not return an iterator

<!-- snapshot-diagnostics -->

```py
class Bad:
    def __iter__(self) -> int:
        return 42

# error: [not-iterable]
for x in Bad():
    reveal_type(x)  # revealed: Unknown
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

<!-- snapshot-diagnostics -->

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

# error: [not-iterable]
for x in Iterable1():
    reveal_type(x)  # revealed: int

# error: [not-iterable]
for y in Iterable2():
    reveal_type(y)  # revealed: Unknown
```

## Possibly missing `__iter__` and bad `__getitem__` method

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
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

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
```

## No `__iter__` method and `__getitem__` is not callable

<!-- snapshot-diagnostics -->

```py
class Bad:
    __getitem__: None = None

# error: [not-iterable]
for x in Bad():
    reveal_type(x)  # revealed: Unknown
```

## Possibly-not-callable `__getitem__` method

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable1():
        # TODO... `int` might be ideal here?
        reveal_type(x)  # revealed: int | Unknown

    # error: [not-iterable]
    for y in Iterable2():
        # TODO... `int` might be ideal here?
        reveal_type(y)  # revealed: int | Unknown
```

## Bad `__getitem__` method

<!-- snapshot-diagnostics -->

```py
class Iterable:
    # invalid because it will implicitly be passed an `int`
    # by the interpreter
    def __getitem__(self, key: str) -> int:
        return 42

# error: [not-iterable]
for x in Iterable():
    reveal_type(x)  # revealed: int
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

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable1():
        reveal_type(x)  # revealed: int

    class Iterable2:
        if flag:
            def __iter__(self) -> Iterator:
                return Iterator()
        else:
            __iter__: None = None

    # error: [not-iterable]
    for x in Iterable2():
        # TODO: `int` would probably be better here:
        reveal_type(x)  # revealed: int | Unknown
```

## Possibly invalid `__next__` method

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable1():
        reveal_type(x)  # revealed: int | str

    # error: [not-iterable]
    for y in Iterable2():
        # TODO: `int` would probably be better here:
        reveal_type(y)  # revealed: int | Unknown
```

## Possibly invalid `__getitem__` methods

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable1():
        # TODO: `str` might be better
        reveal_type(x)  # revealed: str | Unknown

    # error: [not-iterable]
    for y in Iterable2():
        reveal_type(y)  # revealed: str | int
```

## Possibly missing `__iter__` and possibly invalid `__getitem__`

<!-- snapshot-diagnostics -->

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

    # error: [not-iterable]
    for x in Iterable1():
        # TODO: `bytes | str` might be better
        reveal_type(x)  # revealed: bytes | str | Unknown

    # error: [not-iterable]
    for y in Iterable2():
        reveal_type(y)  # revealed: bytes | str | int
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
