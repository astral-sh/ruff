# Comprehensions

## Basic comprehensions

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

# revealed: int
[reveal_type(x) for x in IntIterable()]

class IteratorOfIterables:
    def __next__(self) -> IntIterable:
        return IntIterable()

class IterableOfIterables:
    def __iter__(self) -> IteratorOfIterables:
        return IteratorOfIterables()

# revealed: tuple[int, IntIterable]
[reveal_type((x, y)) for y in IterableOfIterables() for x in y]

# revealed: int
{reveal_type(x): 0 for x in IntIterable()}

# revealed: int
{0: reveal_type(x) for x in IntIterable()}
```

## Nested comprehension

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

# revealed: tuple[int, int]
[[reveal_type((x, y)) for x in IntIterable()] for y in IntIterable()]
```

## Comprehension referencing outer comprehension

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

class IteratorOfIterables:
    def __next__(self) -> IntIterable:
        return IntIterable()

class IterableOfIterables:
    def __iter__(self) -> IteratorOfIterables:
        return IteratorOfIterables()

# revealed: tuple[int, IntIterable]
[[reveal_type((x, y)) for x in y] for y in IterableOfIterables()]
```

## Comprehension with unbound iterable

Iterating over an unbound iterable yields `Unknown`:

```py
# error: [unresolved-reference] "Name `x` used when not defined"
# revealed: Unknown
[reveal_type(z) for z in x]

class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

# error: [not-iterable] "Object of type `int` is not iterable"
# revealed: tuple[int, Unknown]
[reveal_type((x, z)) for x in IntIterable() for z in x]
```

## Starred expressions

Starred expressions must be iterable

```py
class NotIterable: ...

class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

# This is fine:
x = [*Iterable()]

# error: [not-iterable] "Object of type `NotIterable` is not iterable"
y = [*NotIterable()]
```

## Async comprehensions

### Basic

```py
class AsyncIterator:
    async def __anext__(self) -> int:
        return 42

class AsyncIterable:
    def __aiter__(self) -> AsyncIterator:
        return AsyncIterator()

# revealed: @Todo(async iterables/iterators)
[reveal_type(x) async for x in AsyncIterable()]
```

### Invalid async comprehension

This tests that we understand that `async` comprehensions do *not* work according to the synchronous
iteration protocol

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

# revealed: @Todo(async iterables/iterators)
[reveal_type(x) async for x in Iterable()]
```
