# Iterators

## Yield must be iterable

```py
class NotIterable: ...

class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

def generator_function():
    yield from Iterable()
    yield from NotIterable()  # error: "Object of type `NotIterable` is not iterable"
```

## Tuple subclasses

A tuple subclass that inherits `tuple.__iter__` yields its stored elements in order. Converting it
to a tuple or unpacking it into an inline list preserves those elements' precise types.

```py
from collections.abc import Iterator
from typing import Literal

class Inherited(tuple[Literal["a"], Literal["b"]]): ...

inherited = Inherited(("a", "b"))
reveal_type(tuple(inherited))  # revealed: tuple[Literal["a"], Literal["b"]]
reveal_type("b" in [*inherited])  # revealed: Literal[True]
```

An overridden iterator can yield different elements, including none at all. Its return annotation
determines the element type, but does not guarantee the tuple's stored values will be yielded. The
same applies when another subclass inherits that override.

```py
class Empty(tuple[Literal["a"]]):
    def __iter__(self) -> Iterator[Literal["a"]]:
        return iter(())

overridden = Empty(("a",))
reveal_type(tuple(overridden))  # revealed: tuple[Literal["a"], ...]
reveal_type("a" in [*overridden])  # revealed: bool

class AlsoEmpty(Empty): ...

reveal_type(tuple(AlsoEmpty(("a",))))  # revealed: tuple[Literal["a"], ...]
```

Method resolution order determines whether an iterator defined by a mixin overrides
`tuple.__iter__`. When the mixin comes first, iteration uses its return type even if the stored
elements have different values. When `tuple` comes first, iteration retains the tuple's stored
element types.

```py
class ReplacementIterator:
    def __iter__(self) -> Iterator[Literal["replacement"]]:
        yield "replacement"

class MixinFirst(ReplacementIterator, tuple[str]): ...

replaced = MixinFirst(("stored",))
reveal_type(tuple(replaced))  # revealed: tuple[Literal["replacement"], ...]
for element in replaced:
    reveal_type(element)  # revealed: Literal["replacement"]

class TupleFirst(tuple[Literal["replacement"]], ReplacementIterator): ...

reveal_type(tuple(TupleFirst(("replacement",))))  # revealed: tuple[Literal["replacement"]]
```
