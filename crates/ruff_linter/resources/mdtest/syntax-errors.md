# Syntax errors

Syntax errors are reported even when no lint rules are enabled.

```toml
target-version = "py311"

[lint]
select = []
```

## Semantic syntax errors

Some invalid programs parse successfully and are rejected during semantic analysis.

```py
# error: [invalid-syntax] "Duplicate parameter"
def f(x, x):
    pass
```

## Unsupported syntax

Syntax introduced after the configured Python version is also reported.

```py
# error: [invalid-syntax] "Cannot use `type` alias statement on Python 3.11"
type Alias = int
```
