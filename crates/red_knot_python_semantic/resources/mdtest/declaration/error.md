# Errors while declaring

## Violates previous assignment

```py
x = 1
x: str  # error: [invalid-declaration] "Cannot declare type `str` for inferred type `Literal[1]`"
```

## Incompatible declarations

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    x: str
else:
    x: int
x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: str, int"
```

## Partial declarations

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    x: int
x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: Unknown, int"
```

## Incompatible declarations with bad assignment

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    x: str
else:
    x: int

# error: [conflicting-declarations]
# error: [invalid-assignment]
x = b"foo"
```
