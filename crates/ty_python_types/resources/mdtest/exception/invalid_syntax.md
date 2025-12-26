# Exception Handling

## Invalid syntax

```py
try:
    print
except as e:  # error: [invalid-syntax]
    reveal_type(e)  # revealed: Unknown
```
