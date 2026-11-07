# `redirected-rule-selector` (`RUF202`)

```toml
[lint]
preview = true
select = ["RUF202"]
```

## Rule codes and prefixes

`ruff.toml`:

```toml
fixable = ["TCH002", "TC003"]  # snapshot: redirected-rule-selector

[lint]
select = ['TCH', 'F', 'unused-import']  # snapshot: redirected-rule-selector
```

```snapshot
error[RUF202]: Rule selector `TCH002` redirects to `TC002` in `fixable`
 --> src/ruff.toml:1:13
  |
1 | fixable = ["TCH002", "TC003"]  # snapshot: redirected-rule-selector
  |             ^^^^^^
help: Replace with `TC002`
  |
  - fixable = ["TCH002", "TC003"]  # snapshot: redirected-rule-selector
1 + fixable = ["TC002", "TC003"]  # snapshot: redirected-rule-selector
2 |
  |


error[RUF202]: Rule selector `TCH` redirects to `TC` in `lint.select`
 --> src/ruff.toml:4:12
  |
4 | select = ['TCH', 'F', 'unused-import']  # snapshot: redirected-rule-selector
  |            ^^^
help: Replace with `TC`
  |
3 | [lint]
  - select = ['TCH', 'F', 'unused-import']  # snapshot: redirected-rule-selector
4 + select = ['TC', 'F', 'unused-import']  # snapshot: redirected-rule-selector
  |
```

## Renamed rules

`ruff.toml`:

```toml
[lint]
# snapshot: redirected-rule-selector
select = ["missing-return-type-undocumented-public-function"]
```

```snapshot
error[RUF202]: Rule selector `missing-return-type-undocumented-public-function` redirects to `missing-return-type-public-function` in `lint.select`
 --> src/ruff.toml:3:12
  |
3 | select = ["missing-return-type-undocumented-public-function"]
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `missing-return-type-public-function`
  |
2 | # snapshot: redirected-rule-selector
  - select = ["missing-return-type-undocumented-public-function"]
3 + select = ["missing-return-type-public-function"]
  |
```
