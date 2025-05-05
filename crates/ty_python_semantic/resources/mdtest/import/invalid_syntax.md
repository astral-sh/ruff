# Invalid syntax

## Missing module name

```py
from import bar  # error: [invalid-syntax]

reveal_type(bar)  # revealed: Unknown
```

## Invalid nested module import

TODO: This is correctly flagged as an error, but we could clean up the diagnostics that we report.

```py
# TODO: No second diagnostic
# error: [invalid-syntax] "Expected ',', found '.'"
# error: [unresolved-import] "Module `a` has no member `c`"
from a import b.c

# TODO: Should these be inferred as Unknown?
reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(b.c)  # revealed: int
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
c: int = 1
```
