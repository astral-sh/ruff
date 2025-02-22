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

```py
class TestIter:
    def __next__(self) -> int:
        return 42

class Test:
    def __iter__(self) -> TestIter:
        return TestIter()

def _(flag: bool):
    # error: [not-iterable] "Object of type `Test | Literal[42]` may not be iterable because its `__iter__` method is possibly unbound and it doesn't have a `__getitem__` method"
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
    # TODO: Improve error message to state which union variant isn't iterable (https://github.com/astral-sh/ruff/issues/13989)
    # error: "Object of type `Test | Test2` may not be iterable because its `__iter__` method returns an object of type `TestIter | int`, which may not have a `__next__` method"
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

# error: [not-iterable] "Object of type `Test` may not be iterable because its `__iter__` method returns an object of type `TestIter | int`, which may not have a `__next__` method"
for x in Test():
    reveal_type(x)  # revealed: int
```
