# `sys.version_info` for Python 3.13

This test makes sure that `red_knot_test` correctly parses the `target-version` option in a TOML
configuration block. See `sys_version_info.md` for the actual tests for `sys.version_info`.

```toml
[environment]
target-version = "3.13"
```

```py
reveal_type(sys.version_info[:2] == (3, 13))  # revealed: Literal[True]
```
