# `needless-else` (`RUF047`)

```toml
preview = true
lint.select = ["RUF047", "RUF100"]
```

## Explicit `noqa` suppressions

A `noqa` on an empty `else` clause suppresses `RUF047` and counts as used for `RUF100`.

```py
if condition:
    run()
else:  # noqa: RUF047
    pass
```

The suppression also works after an `elif` and with an ellipsis as the empty body.

```py
if first:
    run()
elif second:
    run()
else:  # noqa: RUF047
    ...
```

Loop and `try` statements support the same suppression.

```py
for item in items:
    run(item)
else:  # noqa: RUF047
    pass
```

```py
while condition:
    run()
else:  # noqa: RUF047
    pass
```

```py
try:
    run()
except ValueError:
    recover()
else:  # noqa: RUF047
    pass
```

## Other `noqa` forms

Blanket suppressions and case-insensitive `noqa` directives also count as used.

```py
if condition:
    run()
else:  # noqa
    pass
```

```py
if condition:
    run()
else:  # NoQA: RUF047
    pass
```

A single-line `else` clause can carry its suppression after the body.

```py
if condition:
    run()
else: pass  # noqa: RUF047  # fmt: skip
```

## Unused suppressions

A suppression remains unused when the `else` body does work.

```py
if condition:
    run()
# error: [unused-noqa]
else:  # noqa: RUF047
    recover()
```
