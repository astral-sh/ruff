# `undefined-name` (`F821`)

## Module cache path in Python 3.14

```toml
target-version = "py314"
lint.select = ["F821"]
```

The import system defines `__cached__` in module scope before Python 3.15.

```py
print(__cached__)
```

## Module cache path in Python 3.15

```toml
target-version = "py315"
lint.select = ["F821"]
```

The import system no longer defines `__cached__` in Python 3.15, so `F821` reports uses without a
definition.

```py
print(__cached__)  # error: [undefined-name] "Undefined name `__cached__`"
```
