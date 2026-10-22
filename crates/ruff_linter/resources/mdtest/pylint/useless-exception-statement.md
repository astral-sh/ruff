# `useless-exception-statement` (`PLW0133`)

## Python 3.14

```toml
target-version = "py314"
lint.select = ["PLW0133"]
```

`PythonFinalizationError` is available in Python 3.14, but `ImportCycleError` is not available until
Python 3.15.

```py
import builtins

builtins.PythonFinalizationError("Added in Python 3.13")  # error: [useless-exception-statement]
builtins.ImportCycleError("Added in Python 3.15")
```

## Python 3.15

```toml
target-version = "py315"
lint.select = ["PLW0133"]
```

Both exceptions are available in Python 3.15, so both unused exception statements are reported.

```py
import builtins

builtins.PythonFinalizationError("Added in Python 3.13")  # error: [useless-exception-statement]
builtins.ImportCycleError("Added in Python 3.15")  # error: [useless-exception-statement]
```
