# type[Any]

## Simple

```py
def f(x: type[Any]):
    reveal_type(x)  # revealed: type[Any]
    reveal_type(x.__repr__)  # revealed: Any

class A: ...

f(object)
f(type)
f(A)
```

## Bare type

```py
def f(x: type):
    reveal_type(x)  # revealed: type[Any]
    reveal_type(x.__repr__)  # revealed: Any

class A: ...

f(object)
f(type)
f(A)
```

## type[object] != type[Any]

```py
def f(x: type[object]):
    reveal_type(x)  # revealed: type[object]
    reveal_type(x.__repr__)  # revealed: Literal[__repr__]

class A: ...

f(object)
f(type)
f(A)
```
