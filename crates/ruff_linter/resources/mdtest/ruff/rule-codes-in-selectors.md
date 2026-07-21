# `rule-codes-in-selectors` (`RUF201`)

```toml
[lint]
preview = true
select = ["rule-codes-in-selectors"]
```

## Various quotes

`ruff.toml`:

```toml
[lint]
select = [
    "F401",  # snapshot: rule-codes-in-selectors
    'F402',  # snapshot: rule-codes-in-selectors
    """F403""",  # snapshot: rule-codes-in-selectors
    '''F404''',  # snapshot: rule-codes-in-selectors
]
```

```snapshot
error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:3:6
  |
3 |     "F401",  # snapshot: rule-codes-in-selectors
  |      ^^^^
help: Replace rule code with `unused-import`
  |
2 | select = [
  -     "F401",  # snapshot: rule-codes-in-selectors
3 +     "unused-import",  # snapshot: rule-codes-in-selectors
4 |     'F402',  # snapshot: rule-codes-in-selectors
  |


error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:4:6
  |
4 |     'F402',  # snapshot: rule-codes-in-selectors
  |      ^^^^
help: Replace rule code with `import-shadowed-by-loop-var`
  |
3 |     "F401",  # snapshot: rule-codes-in-selectors
  -     'F402',  # snapshot: rule-codes-in-selectors
4 +     'import-shadowed-by-loop-var',  # snapshot: rule-codes-in-selectors
5 |     """F403""",  # snapshot: rule-codes-in-selectors
  |


error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:5:8
  |
5 |     """F403""",  # snapshot: rule-codes-in-selectors
  |        ^^^^
help: Replace rule code with `undefined-local-with-import-star`
  |
4 |     'F402',  # snapshot: rule-codes-in-selectors
  -     """F403""",  # snapshot: rule-codes-in-selectors
5 +     """undefined-local-with-import-star""",  # snapshot: rule-codes-in-selectors
6 |     '''F404''',  # snapshot: rule-codes-in-selectors
  |


error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:6:8
  |
6 |     '''F404''',  # snapshot: rule-codes-in-selectors
  |        ^^^^
help: Replace rule code with `late-future-import`
  |
5 |     """F403""",  # snapshot: rule-codes-in-selectors
  -     '''F404''',  # snapshot: rule-codes-in-selectors
6 +     '''late-future-import''',  # snapshot: rule-codes-in-selectors
7 | ]
  |
```

## Invalid rule codes

Invalid rule codes are not flagged, including nested quoting issues like `"'F401'"`, but valid codes
in the same selector are still analyzed:

`ruff.toml`:

```toml
[lint]
# snapshot: rule-codes-in-selectors
select = ["'F401'", "F402"]
```

```snapshot
error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:3:22
  |
3 | select = ["'F401'", "F402"]
  |                      ^^^^
help: Replace rule code with `import-shadowed-by-loop-var`
  |
2 | # snapshot: rule-codes-in-selectors
  - select = ["'F401'", "F402"]
3 + select = ["'F401'", "import-shadowed-by-loop-var"]
  |
```

## Invalid selector shapes

Just in case these ever make it past our actual config deserialization, the rule skips over
malformed selectors (e.g. table for `select`, non-table for `per-file-ignores`):

`ruff.toml`:

```toml
[lint]
select = { nested = ["F401"] }
per-file-ignores = ["F401"]
```

## Prefixes and names

Prefixes and rule names are also left alone:

`ruff.toml`:

```toml
[lint]
select = ["F", "unused-import"]
```

## All selectors

Test that we flag all selectors both in the `lint` table and in the deprecated top-level settings:

`ruff.toml`:

```toml
select = ["F401"]  # error: [rule-codes-in-selectors]
extend-select = ["F841"]  # error: [rule-codes-in-selectors]
fixable = ["E501"]  # error: [rule-codes-in-selectors]
extend-fixable = ["UP035"]  # error: [rule-codes-in-selectors]
ignore = ["F401"]  # error: [rule-codes-in-selectors]
extend-ignore = ["F841"]  # error: [rule-codes-in-selectors]
per-file-ignores = { "foo.py" = ["E501"] }  # error: [rule-codes-in-selectors]
extend-per-file-ignores = { "bar.py" = ["UP035"] }  # error: [rule-codes-in-selectors]
unfixable = ["F401"]  # error: [rule-codes-in-selectors]
extend-unfixable = ["F841"]  # error: [rule-codes-in-selectors]
extend-safe-fixes = ["E501"]  # error: [rule-codes-in-selectors]
extend-unsafe-fixes = ["UP035"]  # error: [rule-codes-in-selectors]

[lint]
select = ["F401"]  # error: [rule-codes-in-selectors]
extend-select = ["F841"]  # error: [rule-codes-in-selectors]
fixable = ["E501"]  # error: [rule-codes-in-selectors]
extend-fixable = ["UP035"]  # error: [rule-codes-in-selectors]
ignore = ["F401"]  # error: [rule-codes-in-selectors]
extend-ignore = ["F841"]  # error: [rule-codes-in-selectors]
per-file-ignores = { "foo.py" = ["E501"] }  # error: [rule-codes-in-selectors]
extend-per-file-ignores = { "bar.py" = ["UP035"] }  # error: [rule-codes-in-selectors]
unfixable = ["F401"]  # error: [rule-codes-in-selectors]
extend-unfixable = ["F841"]  # error: [rule-codes-in-selectors]
extend-safe-fixes = ["E501"]  # error: [rule-codes-in-selectors]
extend-unsafe-fixes = ["UP035"]  # error: [rule-codes-in-selectors]
```

## `pyproject.toml`

`pyproject.toml`:

```toml
[tool.ruff]
ignore = ["F401"]  # error: [rule-codes-in-selectors]

[tool.ruff.lint]
select = ["F402"]  # error: [rule-codes-in-selectors]
```

## `unfixable`

Test that `rule-codes-in-selectors` and other TOML-specific lints respect the user's `unfixable`
settings:

```toml
[lint]
preview = true
select = ["rule-codes-in-selectors"]
unfixable = ["rule-codes-in-selectors"]
```

`ruff.toml`:

```toml
# snapshot: rule-codes-in-selectors
lint.select = ["F401"]
```

```snapshot
error[RUF201]: Rule code used instead of name in `lint.select`
 --> src/ruff.toml:2:17
  |
2 | lint.select = ["F401"]
  |                 ^^^^
help: Replace rule code with `unused-import`
```

This should also cover settings like `extend-unsafe-fixes` and `per-file-ignores`, all of which are
handled through the `LintContext`.
