# Subscripts involving type variables

```toml
[environment]
python-version = "3.12"
```

## TypeVar bound/constrained to a tuple/int-literal/bool-literal

The upper bounds of type variables are considered when analysing subscripts.

```py
from typing_extensions import TypeAlias, Literal

ImplicitTuple = tuple[str, int, int]
PEP613Tuple: TypeAlias = tuple[str, int, int]
type PEP695Tuple = tuple[str, int, int]

ImplicitZero = Literal[0]
PEP613Zero: TypeAlias = Literal[0]
type PEP695Zero = Literal[0]

# fmt: off

def f[
    BoundedTupleT: tuple[str, int, bytes],
    ConstrainedTupleT: (tuple[str, int, bytes], tuple[int, bytes, str]),
    BoundedZeroT: Literal[0],
    ConstrainedIntLiteralT: (Literal[0], Literal[1])
](
    tuple_1: BoundedTupleT,
    tuple_2: ConstrainedTupleT,
    zero: BoundedZeroT,
    some_integer: ConstrainedIntLiteralT,
):
    # TODO: would ideally be `tuple[str, int]`
    reveal_type(tuple_1[:2])  # revealed: tuple[str | int | bytes, ...]
    reveal_type(tuple_1[zero])  # revealed: str

    # TODO: ideally this might be `str | int`,
    # but it's hard to do that without introducing false positives elsewhere
    reveal_type(tuple_1[some_integer])  # revealed: str | int | bytes

    reveal_type(tuple_2[:2])  # revealed: tuple[str, int] | tuple[int, bytes]
    reveal_type(tuple_2[zero])  # revealed: str | int
    reveal_type(tuple_2[some_integer])  # revealed: str | int | bytes

# fmt: on
```

## Slicing overlapping constrained sequence types

A value-constrained type variable selects one declared constraint for the entire function call.
Slicing a `list` or a `Sequence` preserves the selected constraint, even though `list` is also a
subtype of `Sequence`.

```py
from collections.abc import Sequence

def slice_sequence[T: (list[int], Sequence[int])](value: T) -> T:
    reveal_type(value[:2])  # revealed: T@slice_sequence
    return value[:2]
```

## Slicing constrained types with distinct implementations

Each constraint's `__getitem__` method must be called with its own receiver. When both methods
return their corresponding constraint, the result preserves the original type variable.

```py
class First:
    def __getitem__(self, index: slice) -> "First":
        return self

class Second:
    def __getitem__(self, index: slice) -> "Second":
        return self

def slice_value[T: (First, Second)](value: T) -> T:
    return value[:2]
```

## Slicing a legacy constrained type variable

Legacy `TypeVar` declarations preserve the selected constraint in the same way as PEP 695 type
parameters.

```toml
[environment]
python-version = "3.10"
```

```py
from collections.abc import Sequence
from typing import TypeVar

T = TypeVar("T", list[int], Sequence[int])

def slice_sequence(value: T) -> T:
    return value[:2]
```

## Slicing a constrained type can change its type

A result cannot retain the constrained type variable when one constraint's slice returns a different
type.

```py
class First:
    def __getitem__(self, index: slice) -> "First":
        return self

class ChangesType:
    def __getitem__(self, index: slice) -> First:
        return First()

def slice_value[T: (First, ChangesType)](value: T) -> T:
    reveal_type(value[:2])  # revealed: First
    # error: [invalid-return-type]
    return value[:2]
```

## Slicing an upper-bounded type variable

An upper-bounded type variable can specialize to a `Sequence` subclass whose slice returns a
different sequence type, so the slice is not guaranteed to preserve the type variable.

```py
from collections.abc import Sequence

def slice_sequence[T: Sequence[int]](value: T) -> T:
    # error: [invalid-return-type]
    return value[:2]
```

## Subscripting an unsupported constrained type

A subscript remains invalid when any declared constraint does not support it.

```py
class Sliceable:
    def __getitem__(self, index: slice) -> "Sliceable":
        return self

def slice_value[T: (Sliceable, int)](value: T) -> None:
    # error: [not-subscriptable]
    value[:2]
```

## TypeVars

```py
from typing import Protocol

class SupportsLessThan(Protocol):
    def __lt__(self, other, /) -> bool: ...

def f[K: SupportsLessThan](dictionary: dict[K, int], key: K):
    reveal_type(dictionary[key])  # revealed: int
```

## ParamSpecs

```py
from typing import Callable

def decorator[**P, T](func: Callable[P, T]) -> Callable[P, T]:
    def inner(*args: P.args, **kwargs: P.kwargs) -> T:
        if len(args) > 0:
            # error: [invalid-assignment]
            args = args[1:]

        # `func` requires the full `ParamSpec` passed into `decorator`,
        # but here the first argument is skipped, so we should possibly emit an error here:
        return func(*args, **kwargs)
    return inner
```
