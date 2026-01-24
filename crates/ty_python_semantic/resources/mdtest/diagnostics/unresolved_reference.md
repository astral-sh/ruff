# Diagnostics for unresolved references

## New builtin used on old Python version

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.9"
```

```py
aiter  # error: [unresolved-reference]
```

## Typing builtin has Info help

A special diagnostic is emitted when using a deprecated alias from Typing that is builtin in this
version of Python. (full diagnostic captured in snapshot)

### Info present in Python 3.9+

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.9"
```

```py
foo: List[int]  # error: [unresolved-reference]
bar: Type  # error: [unresolved-reference]
```

### Info not present before Python 3.9

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.8"
```

```py
foo: List[int]  # error: [unresolved-reference]
bar: Type  # error: [unresolved-reference]
```
