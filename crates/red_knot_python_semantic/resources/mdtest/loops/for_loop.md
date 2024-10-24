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

reveal_type(x)  # revealed: Unbound | int
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

reveal_type(x)  # revealed: Unbound | int
```

## With heterogeneous tuple

```py
for x in (1, "a", b"foo"):
    pass

reveal_type(x)  # revealed: Unbound | Literal[1] | Literal["a"] | Literal[b"foo"]
```

## With non-callable iterator

```py
class NotIterable:
    if flag:
        __iter__ = 1
    else:
        __iter__ = None

for x in NotIterable():  # error: "Object of type `NotIterable` is not iterable"
    pass

reveal_type(x)  # revealed: Unbound | Unknown
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
    __iter__ = None

for x in NotIterable():  # error: "Object of type `NotIterable` is not iterable"
    pass
```
