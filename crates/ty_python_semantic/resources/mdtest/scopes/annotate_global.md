# `__annotate__` as an implicit global is version-gated (Py3.14+)

## Absent before 3.14

```toml
[environment]
python-version = "3.13"
```

```py
# error: [unresolved-reference]
reveal_type(__annotate__)  # revealed: Unknown
```

## Present in 3.14+

```toml
[environment]
python-version = "3.14"
```

```py
reveal_type(__annotate__)  # revealed: Any
```
