# For loops

## Basic `for` loop

```py path=package/basic_for_loop.py
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

```py path=package/public.py
from .basic_for_loop import x # error: [unresolved-import]

reveal_type(x)  # revealed: int
```

## With previous definition

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
    x = 'foo'

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
    x = 'foo'

reveal_type(x)  # revealed: int | Literal["foo"]
```

## With old-style iteration protocol

```py path=package/without_oldstyle_iteration_protocol.py
class OldStyleIterable:
    def __getitem__(self, key: int) -> int:
        return 42

for x in OldStyleIterable():
    pass

reveal_type(x)  # revealed: Unbound | int
```

```py path=package/public.py
from .without_oldstyle_iteration_protocol import x # error: [unresolved-import]

reveal_type(x)  # revealed: int
```

## With heterogeneous tuple

```py path=package/with_heterogeneous_tuple.py
for x in (1, 'a', b'foo'):
    pass

reveal_type(x)  # revealed: Unbound | Literal[1] | Literal["a"] | Literal[b"foo"]
```

```py path=package/public.py
from .with_heterogeneous_tuple import x # error: [unresolved-import]

reveal_type(x)  # revealed: Literal[1] | Literal["a"] | Literal[b"foo"]
```

## With non-callable iterator

```py path=with_noncallable_iterator/with_noncallable_iterator.py
class NotIterable:
    if flag:
        __iter__ = 1
    else:
        __iter__ = None

for x in NotIterable(): # error: "Object of type `NotIterable` is not iterable"
    pass

reveal_type(x)  # revealed: Unbound | Unknown
```

```py path=with_noncallable_iterator/with_noncallable_iterator.py
from .with_noncallable_iterator import x

reveal_type(x)  # revealed: Unknown | int
```

## Invalid iterable

```py
nonsense = 123
for x in nonsense: # error: "Object of type `Literal[123]` is not iterable"
    pass
```

## New over old style iteration protocol

```py
class NotIterable:
    def __getitem__(self, key: int) -> int:
        return 42

    __iter__ = None

for x in NotIterable(): # error: "Object of type `NotIterable` is not iterable"
    pass
```
