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
reveal_type(b.c)  # revealed: Literal[1]
```

```py path=a/__init__.py
```

```py path=a/b.py
c = 1
```
