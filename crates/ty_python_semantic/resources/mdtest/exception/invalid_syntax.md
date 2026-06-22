# Exception Handling

## Invalid syntax

```py
state = 1

try:
    print
except as e:  # error: [invalid-syntax]
    reveal_type(e)  # revealed: Unknown
    state = "handled"

reveal_type(state)  # revealed: Literal[1, "handled"]
```
