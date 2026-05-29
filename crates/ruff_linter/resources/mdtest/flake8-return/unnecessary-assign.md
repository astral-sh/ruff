# `unnecessary-assign` (`RET504`)

```toml
lint.select = ["RET504"]
```

RET504 only fires when the assigned binding has no reference after the `return` expression, since
a `finally` suite (or, conservatively, an `except` handler) is read after the `return`.

## Variable read in the enclosing `finally`

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

A closure captured in `finally` reads the name from another scope:

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

Outer `finally` reads the name across a nested `try`:

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

The outer `finally` runs after a `return` in an inner `finally`:

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

## Augmented assignment in `finally` reads the name

```py
def f():
    try:
        x = foo()
        return x
    finally:
        x += 1
```

```py
def f():
    try:
        x = foo()
        return x
    finally:
        if cond():
            x += 1
```

## `del` of the name in `finally`

Removing the assignment would leave the name unbound, so `del x` would raise `UnboundLocalError`:

```py
def f():
    try:
        x = foo()
        return x
    finally:
        del x
```

```py
def f():
    try:
        x = foo()
        return x
    finally:
        if cond():
            del x
```

## A read after a `finally` rebind still suppresses

Distinguishing a rebind that kills the value from a plain read needs control-flow analysis, so we
conservatively treat the later read as observing the assignment:

```py
def f():
    try:
        x = foo()
        return x
    finally:
        x = "done"
        log(x)
```

```py
def f():
    try:
        x = foo()
        return x
    finally:
        x: str = "done"
        log(x)
```

```py
def f():
    try:
        x = foo()
        return x
    finally:
        x, _ = ("done", 0)
        log(x)
```

## A read in an `except` handler suppresses

```py
def f():
    result = None
    try:
        result = compute()
        return result
    except Exception as e:
        log(result)
```

## `finally` doesn't read the name

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        log("done")
```

```py
def f():
    try:
        x = foo()
        return x  # error: [unnecessary-assign]
    finally:
        x = "done"
```

## Assignment and return both inside `finally`

```py
def f():
    try:
        pass
    finally:
        x = foo()
        return x  # error: [unnecessary-assign]
```

## `return` in an `except` handler with no later read

```py
def f():
    try:
        entry = fetch()
    except AlreadyExists:
        entry = lookup()
        result = to_dict(entry)
        return result  # error: [unnecessary-assign]
```
