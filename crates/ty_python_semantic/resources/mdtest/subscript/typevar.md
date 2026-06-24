# Subscripts involving type variables

## TypeVar bound/constrained to a tuple/int-literal/bool-literal

The upper bounds of type variables are considered when analysing subscripts.

```toml
[environment]
python-version = "3.12"
```

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

    # TODO: would ideally be `tuple[str, int] | tuple[int, bytes]`
    reveal_type(tuple_2[:2])  # revealed: tuple[str | int | bytes, ...]
    reveal_type(tuple_2[zero])  # revealed: str | int
    reveal_type(tuple_2[some_integer])  # revealed: str | int | bytes

# fmt: on
```

## TypeVars

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol

class SupportsLessThan(Protocol):
    def __lt__(self, other, /) -> bool: ...

def f[K: SupportsLessThan](dictionary: dict[K, int], key: K):
    reveal_type(dictionary[key])  # revealed: int
```

## ParamSpecs

```toml
[environment]
python-version = "3.12"
```

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
