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
