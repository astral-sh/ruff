# Narrowing in `while` loops

We only make sure that narrowing works for `while` loops in general, we do not exhaustively test all
narrowing forms here, as they are covered in other tests.

## Basic example

```py
def next_item() -> int | None: ...

x = next_item()

while x is not None:
    reveal_type(x)  # revealed: int
    x = next_item()
else:
    reveal_type(x)  # revealed: None
```
