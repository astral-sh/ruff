# Shadwing declaration

## Shadow after incompatible declarations is OK

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()

if flag:
    x: str
else:
    x: int
x: bytes = b"foo"
```
