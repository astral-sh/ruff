# `invalid-pyproject-toml` (`RUF200`)

```toml
[lint]
select = ["RUF200"]
```

`pyproject.toml`:

```toml
[project]
name = 1 # snapshot: invalid-pyproject-toml
```

```snapshot
error[RUF200]: Failed to parse pyproject.toml: invalid type: integer `1`, expected a string
 --> src/pyproject.toml:2:8
  |
2 | name = 1 # snapshot: invalid-pyproject-toml
  |        ^
  |
```
