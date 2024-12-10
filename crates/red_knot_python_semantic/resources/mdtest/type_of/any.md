# type[Any]

## Simple

```py
def f(x: type[Any]):
    reveal_type(x)  # revealed: type[Any]
    # TODO: could be `<object.__repr__ type> & Any`
    reveal_type(x.__repr__)  # revealed: Any

class A: ...

x: type[Any] = object
x: type[Any] = type
x: type[Any] = A
```

## Bare type

```py
def f(x: type):
    reveal_type(x)  # revealed: type[Any]
    # TODO: could be `<object.__repr__ type> & Any`
    reveal_type(x.__repr__)  # revealed: Any

class A: ...

x: type[Any] = object
x: type[Any] = type
x: type[Any] = A
```

## type[object] != type[Any]

```py
def f(x: type[object]):
    reveal_type(x)  # revealed: type[object]
    # TODO: bound method types
    reveal_type(x.__repr__)  # revealed: Literal[__repr__]

class A: ...

x: type[Any] = object
x: type[Any] = type
x: type[Any] = A
```
