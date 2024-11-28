## After if-else statements, narrowing has an effect if variable is mutated

```py
def optional_int() -> int | None: ...

x = optional_int()
y = optional_int()

if x is None:
    x = 10
else:
    pass

reveal_type(x)  # revealed: int
```
