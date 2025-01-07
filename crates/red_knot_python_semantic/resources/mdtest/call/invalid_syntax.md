# Invalid signatures

## Multiple arguments with the same name

We always map a keyword argument to the first parameter of that name.

```py
# error: [invalid-syntax] "Duplicate parameter "x""
def f(x: int, x: str) -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x` of function `f`"
# error: 18 [parameter-already-assigned] "Multiple values provided for parameter `x` of function `f`"
reveal_type(f(1, x=2))  # revealed: int
```

## Positional after non-positional

When parameter kinds are given in an invalid order, we emit a diagnostic and implicitly reorder them
to the valid order:

```py
# error: [invalid-syntax] "Parameter cannot follow var-keyword parameter"
def f(**kw: int, x: str) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal[1]` cannot be assigned to parameter 1 (`x`) of function `f`; expected type `str`"
reveal_type(f(1))  # revealed: int
```

## Non-defaulted after defaulted

We emit a syntax diagnostic for this, but it doesn't cause any problems for binding.

```py
# error: [invalid-syntax] "Parameter without a default cannot follow a parameter with a default"
def f(x: int = 1, y: str) -> int:
    return 1

reveal_type(f(y="foo"))  # revealed: int
# error: [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter 1 (`x`) of function `f`; expected type `int`"
# error: [missing-argument] "No argument provided for required parameter `y` of function `f`"
reveal_type(f("foo"))  # revealed: int
```
