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
from typing_extensions import reveal_type

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

## Union type as iterable where one union element has no `__iter__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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
from typing_extensions import reveal_type

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

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter | int:
        return TestIter()

# error: [not-iterable]
for x in Test():
    reveal_type(x)  # revealed: int
```

## Possibly-not-callable `__iter__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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

    # error: [not-iterable]
    for x in Iterable1():
        # TODO... `int` might be ideal here?
        reveal_type(x)  # revealed: int | Unknown

    # error: [not-iterable]
    for y in Iterable2():
        # TODO... `int` might be ideal here?
        reveal_type(y)  # revealed: int | Unknown
```

## `__iter__` method with a bad signature

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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
from typing_extensions import reveal_type

class Bad:
    def __iter__(self) -> int:
        return 42

# error: [not-iterable]
for x in Bad():
    reveal_type(x)  # revealed: Unknown
```

## `__iter__` returns an object with a possibly unbound `__next__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

def _(flag: bool):
    class Iterator:
        if flag:
            def __next__(self) -> int:
                return 42

    class Iterable:
        def __iter__(self) -> Iterator:
            return Iterator()

    # error: [not-iterable]
    for x in Iterable():
        reveal_type(x)  # revealed: int
```

## `__iter__` returns an iterator with an invalid `__next__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

class Iterator:
    def __next__(self, extra_arg) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

# error: [not-iterable]
for x in Iterable():
    reveal_type(x)  # revealed: int
```

## Possibly unbound `__iter__` and bad `__getitem__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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
            return 42

    # error: [not-iterable]
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
```

## Possibly unbound `__iter__` and possibly unbound `__getitem__`

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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
                return 42

    # error: [not-iterable]
    for x in Iterable():
        reveal_type(x)  # revealed: int | bytes
```

## No `__iter__` method and `__getitem__` is not callable

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

class Bad:
    __getitem__: None = None

# error: [not-iterable]
for x in Bad():
    reveal_type(x)  # revealed: Unknown
```

## Possibly-not-callable `__getitem__` method

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

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
from typing_extensions import reveal_type

class Iterable:
    # invalid because it will implicitly be passed an `int`
    # by the interpreter
    def __getitem__(self, key: str) -> int:
        return 42

# error: [not-iterable]
for x in Iterable():
    reveal_type(x)  # revealed: int
```

## Possibly unbound `__iter__` but definitely bound `__getitem__`

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
