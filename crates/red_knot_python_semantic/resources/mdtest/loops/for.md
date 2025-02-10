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

    for x in NotIterable():  # error: "Object of type `NotIterable` is not iterable"
        pass

    # revealed: Unknown
    # error: [possibly-unresolved-reference]
    reveal_type(x)
```

## Invalid iterable

```py
nonsense = 123
for x in nonsense:  # error: "Object of type `Literal[123]` is not iterable"
    pass
```

## New over old style iteration protocol

```py
class NotIterable:
    def __getitem__(self, key: int) -> int:
        return 42
    __iter__: None = None

for x in NotIterable():  # error: "Object of type `NotIterable` is not iterable"
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
class TestIter:
    def __next__(self) -> int | Exception:
        return 42

class TestIter2:
    def __next__(self) -> str | tuple[int, int]:
        return "42"

class TestIter3:
    def __next__(self) -> bytes:
        return b"42"

class TestIter4:
    def __next__(self) -> memoryview:
        return memoryview(b"42")

class Test:
    def __iter__(self) -> TestIter | TestIter2:
        return TestIter()

class Test2:
    def __iter__(self) -> TestIter3 | TestIter4:
        return TestIter3()

def _(flag: bool):
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: int | Exception | str | tuple[int, int] | bytes | memoryview
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
    # error: [not-iterable] "Object of type `Test | Literal[42]` is not iterable because its `__iter__` method is possibly unbound"
    for x in Test() if flag else 42:
        reveal_type(x)  # revealed: int
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
    # error: "Object of type `Test | Test2` is not iterable"
    for x in Test() if flag else Test2():
        reveal_type(x)  # revealed: Unknown
```

## Union type as iterator where one union element has no `__next__` method

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter | int:
        return TestIter()

# error: [not-iterable] "Object of type `Test` is not iterable"
for x in Test():
    reveal_type(x)  # revealed: Unknown
```
