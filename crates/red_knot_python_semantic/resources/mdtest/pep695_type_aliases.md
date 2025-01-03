# PEP 695 type aliases

PEP 695 type aliases are only available in Python 3.12 and later:

```toml
[environment]
python-version = "3.12"
```

## Basic

```py
type IntOrStr = int | str

reveal_type(IntOrStr)  # revealed: typing.TypeAliasType
reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]

x: IntOrStr = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: int | str
```

## `__value__` attribute

```py
type IntOrStr = int | str

# TODO: This should either fall back to the specified type from typeshed,
# which is `Any`, or be the actual type of the runtime value expression
# `int | str`, i.e. `types.UnionType`.
reveal_type(IntOrStr.__value__)  # revealed: @Todo(instance attributes)
```

## Invalid assignment

```py
type OptionalInt = int | None

# error: [invalid-assignment]
x: OptionalInt = "1"
```

## Type aliases in type aliases

```py
type IntOrStr = int | str
type IntOrStrOrBytes = IntOrStr | bytes

x: IntOrStrOrBytes = 1

def f() -> None:
    reveal_type(x)  # revealed: int | str | bytes
```

## Aliased type aliases

```py
type IntOrStr = int | str
MyIntOrStr = IntOrStr

x: MyIntOrStr = 1

# error: [invalid-assignment]
y: MyIntOrStr = None
```

## Generic type aliases

```py
type ListOrSet[T] = list[T] | set[T]

# TODO: Should be `tuple[typing.TypeVar | typing.ParamSpec | typing.TypeVarTuple, ...]`,
# as specified in the `typeshed` stubs.
reveal_type(ListOrSet.__type_params__)  # revealed: @Todo(instance attributes)
```
