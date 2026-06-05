# `TypeVarTuple` in exception handlers

```toml
[environment]
python-version = "3.10"
```

Semantic analysis should not panic when syntax recovery exposes a PEP 695 `TypeVarTuple` as an
exception handler type on an older Python version.

```py
# error: [invalid-syntax]
def regular[*Ts]() -> None:
    try:
        pass
    # error: [invalid-exception-caught] "Invalid object caught in an exception handler: Object has type `TypeVarTuple`"
    except Ts:
        pass

# error: [invalid-syntax]
def tuple_handler[*Ts]() -> None:
    try:
        pass
    # error: [invalid-exception-caught]
    except (Ts,):
        pass

# error: [invalid-syntax]
def starred[*Us]() -> None:
    try:
        pass
    # error: 11 [invalid-syntax] "Cannot use `except*` on Python 3.10 (syntax was added in Python 3.11)"
    # error: [invalid-exception-caught] "Invalid object caught in an exception handler: Object has type `TypeVarTuple`"
    except* Us:
        pass
```
