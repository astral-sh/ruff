# Function shadowing

## Parameter

Parameter `x` of type `str` is shadowed and reassigned with a new `int` value inside the function.
No diagnostics should be generated.

```py path=a.py
def f(x: str):
    x: int = int(x)
```

## Implicit error

```py path=a.py
def f(): ...

f = 1  # error: "Implicit shadowing of function `f`; annotate to make it explicit if this is intentional"
```

## Explicit shadowing

```py path=a.py
def f(): ...

f: int = 1
```
