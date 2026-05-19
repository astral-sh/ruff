# PEP 695 `TypeVarTuple`

```toml
[environment]
python-version = "3.12"
```

## Definition

A PEP 695 type variable tuple is introduced with a starred type parameter.

```py
def f[*Ts]() -> None:
    reveal_type(Ts)  # revealed: TypeVarTuple
```

## Generic Classes

### Explicit specialization

```py
class Array[*Ts]:
    def shape(self) -> tuple[*Ts]:
        raise NotImplementedError

class Sandwich[T, *Ts, U]:
    def parts(self) -> tuple[T, *Ts, U]:
        raise NotImplementedError

def f(
    array: Array[int, str],
    empty: Array[()],
    sandwich1: Sandwich[int, bool, str],
    sandwich2: Sandwich[int, str],
) -> None:
    reveal_type(array)  # revealed: Array[int, str]
    reveal_type(array.shape())  # revealed: tuple[int, str]
    reveal_type(empty)  # revealed: Array[()]
    reveal_type(empty.shape())  # revealed: tuple[()]
    reveal_type(sandwich1.parts())  # revealed: tuple[int, bool, str]
    reveal_type(sandwich2.parts())  # revealed: tuple[int, str]
```

### Constructor inference

```py
class Array[*Ts]:
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

def f(i: int, s: str) -> None:
    reveal_type(Array((i, s)))  # revealed: Array[int, str]
    reveal_type(Array(()))  # revealed: Array[()]
```

### Variance inference

PEP 695 type variable tuples infer variance from how the class uses them.

```py
class CovariantArray[*Ts]:
    def get(self) -> tuple[*Ts]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

class ContravariantArray[*Ts]:
    def set(self, value: tuple[*Ts]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

class InvariantArray[*Ts]:
    values: tuple[*Ts]

invariant_out: InvariantArray[object] = InvariantArray[int]()  # error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # error: [invalid-assignment]
```

## Functions

### Tuple arguments and returns

```py
def echo[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x

def with_prefix[T, *Ts](x: T, y: tuple[*Ts]) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_suffix[*Ts, U](x: tuple[*Ts], y: U) -> tuple[*Ts, U]:
    raise NotImplementedError

def with_both[T, *Ts, U](x: tuple[T, *Ts, U]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(echo((i, s)))  # revealed: tuple[int, str]
    reveal_type(echo(()))  # revealed: tuple[()]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[str]
```

### Starred variadic parameters

```py
def args_to_tuple[*Ts](*args: *Ts) -> tuple[*Ts]:
    raise NotImplementedError

def first_and_rest[T, *Ts](first: T, *rest: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(args_to_tuple(i, s))  # revealed: tuple[int, str]
    reveal_type(args_to_tuple())  # revealed: tuple[()]
    reveal_type(first_and_rest(i, s, b))  # revealed: tuple[int, str, bool]
```

## Type Aliases

### Variadic aliases

```py
type Identity[*Ts] = tuple[*Ts]
type Prefix[*Ts] = tuple[int, *Ts]
type Suffix[*Ts] = tuple[*Ts, str]
type Sandwich[T, *Ts, U] = tuple[T, *Ts, U]

def f(
    identity: Identity[int, str],
    empty: Identity[()],
    prefix: Prefix[bool, str],
    suffix: Suffix[int, bool],
    sandwich1: Sandwich[int, bool, str],
    sandwich2: Sandwich[int, str],
) -> None:
    reveal_type(identity)  # revealed: tuple[int, str]
    reveal_type(empty)  # revealed: tuple[()]
    reveal_type(prefix)  # revealed: tuple[int, bool, str]
    reveal_type(suffix)  # revealed: tuple[int, bool, str]
    reveal_type(sandwich1)  # revealed: tuple[int, bool, str]
    reveal_type(sandwich2)  # revealed: tuple[int, str]
```

### Unpacked tuple type arguments

```py
type Prefix[*Ts] = tuple[int, *Ts]

def f(x: Prefix[*tuple[str, bool]], y: Prefix[*tuple[str, ...]]) -> None:
    reveal_type(x)  # revealed: tuple[int, str, bool]
    reveal_type(y)  # revealed: tuple[int, *tuple[str, ...]]
```

## Defaults

### PEP 695 defaults

```toml
[environment]
python-version = "3.13"
```

```py
class Array[*Ts = *tuple[int, str]]:
    def shape(self) -> tuple[*Ts]:
        raise NotImplementedError

def f(default: Array, explicit: Array[bool]) -> None:
    reveal_type(default.shape())  # revealed: tuple[int, str]
    reveal_type(explicit.shape())  # revealed: tuple[bool]
```

### Defaults cannot be followed by type parameters with defaults

```toml
[environment]
python-version = "3.13"
```

```py
# error: [invalid-type-variable-default]
class Invalid[*Ts, T = int]: ...
```

## Invalid Forms

### Must always be unpacked

```py
def invalid[*Ts](x: Ts) -> None: ...  # error: [invalid-type-form]
def invalid_args[*Ts](*args: Ts) -> None: ...  # error: [invalid-type-form]
def valid[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```
