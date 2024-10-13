# Loops

## While

### Basic While Loop

```py
x = 1
while flag:
    x = 2

reveal_type(x)  # revealed: Literal[1, 2]
```

### While with else (no break)

```py
x = 1
while flag:
    x = 2
else:
    y = x
    x = 3

reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: Literal[1, 2]
```

### While with Else (may break)

```py
x = 1
y = 0
while flag:
    x = 2
    if flag2:
        y = 4
        break
else:
    y = x
    x = 3

reveal_type(x)  # revealed: Literal[2, 3]
reveal_type(y)  # revealed: Literal[1, 2, 4]
```

## For

### Basic For Loop

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

### With previous definition

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

x = 'foo'

for x in IntIterable():
    pass

reveal_type(x)  # revealed: Literal["foo"] | int
```

### With Else (no break)

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
    x = 'foo'

reveal_type(x)  # revealed: Literal["foo"]
```

### May break

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
    x = 'foo'

reveal_type(x)  # revealed: int | Literal["foo"]
```

### With old-style iteration protocol

```py
class OldStyleIterable:
    def __getitem__(self, key: int) -> int:
        return 42

for x in OldStyleIterable():
    pass

reveal_type(x)  # revealed: Unbound | int
```

### With heterogeneous tuple

```py
for x in (1, 'a', b'foo'):
    pass

reveal_type(x)  # revealed: Unbound | Literal[1] | Literal["a"] | Literal[b"foo"]
```

### With non-callable iterator

```py
class NotIterable:
    if flag:
        __iter__ = 1
    else:
        __iter__ = None

for x in NotIterable(): # error: "Object of type `NotIterable` is not iterable"
    pass

reveal_type(x)  # revealed: Unbound | Unknown
```

### Invalid iterable

```py
nonsense = 123
for x in nonsense: # error: "Object of type `Literal[123]` is not iterable"
    pass
```

### New over old style iteration protocol

```py
class NotIterable:
    def __getitem__(self, key: int) -> int:
        return 42

    __iter__ = None

for x in NotIterable(): # error: "Object of type `NotIterable` is not iterable"
    pass
```

### Yield must be iterable

```py
class NotIterable: pass

class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator: ...

def generator_function():
    yield from Iterable()
    yield from NotIterable() # error: "Object of type `NotIterable` is not iterable"
```

### Async

This tests that we understand that `async` for loops do not work according to the synchronous iteration protocol.

#### TODO: Invalid async for loop

TODO: We currently return `Todo` for all `async for` loops, including loops that have invalid syntax.

```py
async def foo():
    class Iterator:
        def __next__(self) -> int:
            return 42

    class Iterable:
        def __iter__(self) -> Iterator:
            return Iterator()

    async for x in Iterator():
        pass

    reveal_type(x)  # revealed: Unbound | @Todo
```

#### TODO: Basic async for loop

TODO(Alex): async iterables/iterators!

```py
async def foo():
    class IntAsyncIterator:
        async def __anext__(self) -> int:
            return 42

    class IntAsyncIterable:
        def __aiter__(self) -> IntAsyncIterator:
            return IntAsyncIterator()

    async for x in IntAsyncIterable():
        pass

    reveal_type(x)  # revealed: Unbound | @Todo
```
