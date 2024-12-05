# `sys.version_info` for Python 3.13

This test makes sure that we correctly parse the `target-version` configuration option. See
`sys_version_info.md` for the actual tests for `sys.version_info`.

```toml
[tool.knot.environment]
target-version = "3.13"
```

```py
reveal_type(sys.version_info[:2] == (3, 13))  # revealed: Literal[True]
```
