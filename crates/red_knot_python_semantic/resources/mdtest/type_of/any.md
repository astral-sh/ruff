# type[Any]

## Simple

```py
def f(x: type[Any]):
    reveal_type(x)  # revealed: type[Any]

class A: ...

f(object)
f(type)
f(A)
```

## Bare type

```py
def f(x: type):
    reveal_type(x)  # revealed: type[Any]

class A: ...

f(object)
f(type)
f(A)
```
