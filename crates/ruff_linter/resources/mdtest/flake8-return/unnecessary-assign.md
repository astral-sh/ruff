# `unnecessary-assign` (`RET504`)

```toml
lint.select = ["RET504"]
```

## Variable read in the enclosing `finally` (no diagnostic)

The `finally` clause runs after the `return`, so the assignment is observable
even though it looks redundant.

```py
def f():
    out = ""
    try:
        out = foo()
        return out
    except Exception as e:
        out = str(e)
    finally:
        log(out)
```

A closure captured in `finally` counts as observation, conservatively.

```py
def f():
    try:
        x = foo()
        return x
    finally:
        def _cleanup():
            log(x)
        _cleanup()
```

Outer `finally` reads the name across a nested `try`.

```py
def f():
    x = ""
    try:
        try:
            x = foo()
            return x
        except:
            pass
    finally:
        log(x)
```

When the `return` lives in an inner `finally`, the outer `finally` still
runs after it and observes the assignment.

```py
def f():
    x = ""
    try:
        try:
            pass
        finally:
            x = foo()
            return x
    finally:
        log(x)
```

## `x += 1` in `finally` reads `x` (no diagnostic)

An augmented assignment loads the target before writing it, but the AST
encodes the target with `Store` context. RET504 must still recognize it
as a read.

```py
def f():
    try:
        x = foo()
        return x
    finally:
        x += 1
```

Nested augmented assignment is also a read:

```py
def f():
    try:
        x = foo()
        return x
    finally:
        if cond():
            x += 1
```

## `finally` rebinds the name before reading it (RET504 fires)

An unconditional top-level rebind in `finally` kills the `try`'s value, so
later reads in `finally` no longer observe it and the assignment is
genuinely redundant.

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        x = "done"
        log(x)
```

Annotated assignment with a value also rebinds:

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        x: str = "done"
        log(x)
```

Tuple-unpacking assignment rebinds the targets it lists:

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        x, _ = ("done", 0)
        log(x)
```

## `finally` doesn't read the name (RET504 fires)

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        log("done")
```

Write-only in `finally` (no read of `x` before the rebind):

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        x = "done"
```

## Assignment and return both inside `finally`

The currently-executing `finally` has no second pass, so its body can't
re-read the assignment after the return.

```py
def f():
    try:
        pass
    finally:
        x = foo()
        return x  # error: [unnecessary-assign]
```

## `except` handler reads the name (RET504 fires)

The handler is an alternative path: if it runs, the `try` assignment never
completed, so removing the assignment doesn't change what the handler
observes.

```py
def f():
    result = None
    try:
        result = compute()
        return result  # error: [unnecessary-assign]
    except Exception as e:
        log(result)
```

Return inside an `except` handler with no `finalbody`:

```py
def f():
    try:
        entry = fetch()
    except AlreadyExists:
        entry = lookup()
        result = to_dict(entry)
        return result  # error: [unnecessary-assign]
```
