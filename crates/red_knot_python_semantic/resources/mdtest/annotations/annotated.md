# `Annotated`

`Annotated` attachs arbitrary metadata to a given type.

## Usages

`Annotated[T, ...]` is equivalent to `T`: All metadata arguments are simply ignored.

```py
from typing_extensions import Annotated

def f(x: Annotated[int, "foo"]):
    reveal_type(x)  # revealed: int
```

## Parametrization

It is invalid to parametrize `Annotated` with less than two arguments.

```py
from typing_extensions import Annotated

def _(x: Annotated):
    reveal_type(x)  # revealed: Unknown

# error: [invalid-type-parameter]
def _(x: Annotated[()]):
    reveal_type(x)  # revealed: Unknown

# `Annotated[T]` is invalid and will raise an error at runtime,
# but we treat it the same as `T` to provide better diagnostics later on.
# The subscription itself is still reported, regardless.
# error: [invalid-type-parameter]
def _(x: Annotated[int]):
    reveal_type(x)  # revealed: int
```

## Inheritance

### Correctly parametrized

Inheriting from `Annotated[T, ...]` is equivalent to inheriting from `T` itself.

```py
from typing_extensions import Annotated

# TODO: False positive
# error: [invalid-base]
class C(Annotated[int, "foo"]): ...

# TODO: Should be `tuple[Literal[C], Literal[int], Literal[object]]`
reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Unknown, Literal[object]]
```

### Not parametrized

```py
from typing_extensions import Annotated

# error: [invalid-base]
class C(Annotated): ...

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Unknown, Literal[object]]
```
