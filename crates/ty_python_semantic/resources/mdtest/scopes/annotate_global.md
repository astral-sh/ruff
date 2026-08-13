# `__annotate__` as an implicit global is version-gated (Py3.14+)

## Absent before 3.14

`__annotate__` is never present in the global namespace on Python \<3.14.

```toml
[environment]
python-version = "3.13"
```

```py
# error: [unresolved-reference]
reveal_type(__annotate__)  # revealed: Unknown
```

## Present in 3.14+

The `__annotate__` global may be present in Python 3.14, but only if at least one global symbol in
the module is annotated (e.g. `x: int` or `x: int = 42`). Currently we model `__annotate__` as
always being possibly unbound on Python 3.14+.

```toml
[environment]
python-version = "3.14"
```

```py
# error: [possibly-unresolved-reference]
reveal_type(__annotate__)  # revealed: (format: int, /) -> dict[str, Any]
```
