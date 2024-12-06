# type[Any]

## Simple

```py
class A: ...

def f() -> type[Any]:
    return A

reveal_type(f())  # revealed: type[Any]
```
