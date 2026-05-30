# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## Repeated arguments with side effects

A `str.format` argument is evaluated once, but an f-string re-evaluates it on every interpolation. So
when an argument is used more than once and can have a side effect, the call is left unconverted to
avoid running that side effect twice. This used to cover only call expressions; it now covers any
side-effecting expression, such as a subscript or a walrus binding.

```py
def foo(): ...


d = {}

"{x} {x}".format(x=foo())
"{x} {x}".format(x=d["k"])
"{x} {x}".format(x=(y := 1))
```

## A single use is still converted

When the argument is interpolated once, the side effect runs once either way, so the fix is safe.

```py
def foo(): ...


"{x}".format(x=foo())  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:4:1
  |
4 | "{x}".format(x=foo())  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | def foo(): ...
2 |
3 |
  - "{x}".format(x=foo())  # snapshot: f-string
4 + f"{foo()}"  # snapshot: f-string
```
