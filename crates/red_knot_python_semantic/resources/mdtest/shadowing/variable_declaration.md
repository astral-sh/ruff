# Shadwing declaration

## Shadow after incompatible declarations is OK

```py
def _(flag: bool) -> None:
    if flag:
        x: str
    else:
        x: int

    x: bytes = b"foo"
```
