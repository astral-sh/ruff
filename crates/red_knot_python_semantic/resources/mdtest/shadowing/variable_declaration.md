# Shadwing declaration

## Shadow after incompatible declarations is OK

```py
if flag:
    x: str
else:
    x: int
x: bytes = b"foo"
```
