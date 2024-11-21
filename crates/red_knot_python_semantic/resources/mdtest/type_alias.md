# Type aliases

## Basic

```py
type IntOrStr = int | str

reveal_type(IntOrStr)  # revealed: typing.TypeAliasType
reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]
reveal_type(IntOrStr.__value__)  # revealed: int | str

x: IntOrStr = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: int | str
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

reveal_type(IntOrStrOrBytes.__value__)  # revealed: int | str | bytes
```

## Aliased type aliases

```py
type IntOrStr = int | str

MyIntOrStr = IntOrStr

reveal_type(MyIntOrStr.__value__)  # revealed: int | str

x: MyIntOrStr = 1

# error: [invalid-assignment]
y: MyIntOrStr = None
```

## Generic type aliases

```py
type ListOrSet[T] = list[T] | set[T]

# TODO: Should be tuple[typing.TypeVar | typing.ParamSpec | typing.TypeVarTuple, ...]
reveal_type(ListOrSet.__type_params__)  # revealed: @Todo(TypeAliasType __type_params__)
```
