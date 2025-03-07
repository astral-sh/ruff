# Function return type

```py
def f() -> int:
    return 1

def f():
    return

def f() -> str:
    raise NotImplementedError()

def f(cond: bool) -> int | None:
    if cond:
        return 1
    else:
        return

def f(cond: bool) -> str:
    if cond:
        return "a"
    else:
        return "b"

def f(cond: bool) -> str | int:
    if cond:
        return "a"
    else:
        return 1

# error: [invalid-return-type]
def f() -> int:
    1

def f() -> str:
    # error: [invalid-return-type]
    return 1

def f() -> int:
    # error: [invalid-return-type]
    return

def f(cond: bool) -> str:
    if cond:
        return "a"
    else:
        # error: [invalid-return-type]
        return 1

def f(cond: bool) -> str:
    if cond:
        # error: [invalid-return-type]
        return 1
    else:
        # error: [invalid-return-type]
        return 2
```
