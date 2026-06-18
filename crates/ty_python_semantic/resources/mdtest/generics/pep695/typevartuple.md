# PEP 695 `TypeVarTuple`

```toml
environment.python-version = "3.13"
```

## Definition and validation

```py
def definition[*Ts](*args: *Ts) -> tuple[*Ts]:
    reveal_type(Ts)  # revealed: TypeVarTuple
    reveal_type(args)  # revealed: tuple[*Ts@definition]
    return args

class Invalid[*Ts, *Us]:  # error: [invalid-type-form]
    pass
```

## Explicit specialization

```py
class Simple[*Ts]:
    value: tuple[*Ts]

class Between[T, *Ts, U]:
    value: tuple[T, *Ts, U]

reveal_type(Simple[()]().value)  # revealed: tuple[()]
reveal_type(Simple[int, str]().value)  # revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, ...]]().value)  # revealed: tuple[int, ...]
reveal_type(Between[int, bool, bytes, str]().value)  # revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, *tuple[bool, ...], str]().value)  # revealed: tuple[int, *tuple[bool, ...], str]
reveal_type(Between().value)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

## Calls use a gradual pack

TypeVarTuple inference is intentionally deferred. Ordinary type variables in the same signature are
still inferred.

```py
def collect[*Ts](*args: *Ts) -> tuple[*Ts]:
    return args

def mixed[T, *Ts](first: T, *rest: *Ts) -> tuple[T, *Ts]:
    return (first, *rest)

reveal_type(collect())  # revealed: tuple[Unknown, ...]
reveal_type(collect(1, "a"))  # revealed: tuple[Unknown, ...]
reveal_type(mixed(1, "a"))  # revealed: tuple[Literal[1], *tuple[Unknown, ...]]
```

## Callable parameters

An unsolved pack in a callable parameter list is gradual and does not reject a callable solely
because its arity was not inferred.

```py
from typing import Callable

def accepts[*Ts](callback: Callable[[*Ts], None]) -> tuple[*Ts]:
    raise NotImplementedError

def target(first: int, second: str) -> None: ...

reveal_type(accepts(target))  # revealed: tuple[Unknown, ...]
```
