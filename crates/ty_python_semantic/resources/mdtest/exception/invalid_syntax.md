# Exception Handling

## Invalid syntax

```py
try:
    print()
except as e:  # error: [invalid-syntax]
    reveal_type(e)  # revealed: Unknown
```

## Invalid handler syntax does not create an exception path

A parser error in the handler does not make a non-raising `try` suite reach that handler.

```py
state = 0
try:
    state = 1
except as e:  # error: [invalid-syntax]
    state = "unreachable"

reveal_type(state)  # revealed: Literal[1]
```
