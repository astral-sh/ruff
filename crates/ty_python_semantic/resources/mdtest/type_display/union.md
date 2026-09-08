# Display of union types

## Nested unions

Each conditional assignment either resets the value to zero or wraps the previous value in a tuple.
The displayed type retains the zero alternative at every possible nesting depth.

```py
def f(flag: bool):
    value = 0
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    value = 0 if flag else (value,)
    # revealed: Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0] | tuple[Literal[0]]]]]]]]]]]]]]]]]]]]]]]]]
    reveal_type(value)
```
