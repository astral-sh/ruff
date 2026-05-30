# `unnecessary-assign` (`RET504`)

```toml
lint.select = ["RET504"]
```

RET504 is suppressed only when the assigned name is read in an enclosing `finally` suite, which
runs after the `return`. Reads elsewhere (sibling branches, `except` handlers) don't run after the
`return`, so they don't keep the assignment alive.

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

The `finally` also runs after a `return` in the `else` clause:

```py
def f():
    try:
        pass
    except Exception:
        pass
    else:
        x = compute()
        return x
    finally:
        log(x)
```

And after a `return` in an `except` handler:

```py
def f():
    try:
        pass
    except Exception:
        x = recover()
        return x
    finally:
        log(x)
```

The assignment may also come from a `with` body inside the `try`:

```py
def f():
    try:
        with open("f") as fh:
            x = fh.read()
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

A rebind in `finally` makes the assignment redundant, but distinguishing a rebind that kills the
value from a plain read needs control-flow analysis we don't do here. We conservatively treat the
later read as observing the assignment.

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

```py
def f():
    try:
        x = foo()
        return x
    finally:
        if cond():
            x = "done"
        log(x)
```

## A read in an `except` handler fires

An `except` handler is an alternative path: if it runs, the `try` assignment never completed, so
removing the assignment doesn't change what the handler reads.

```py
def f():
    result = None
    try:
        result = compute()
        return result  # error: [unnecessary-assign]
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

## Same name assigned and returned in sibling branches

Each branch's assignment is independently redundant. A later branch reusing the name doesn't
observe an earlier branch's value, so both fire.

```py
def f(cond):
    if cond:
        x = compute()
        return x  # error: [unnecessary-assign]
    else:
        x = other()
        return x  # error: [unnecessary-assign]
```

The same holds when the branches are `try` arms without a `finally`:

```py
def f():
    try:
        x = compute()
        return x  # error: [unnecessary-assign]
    except Exception:
        x = fallback()
        return x  # error: [unnecessary-assign]
```
