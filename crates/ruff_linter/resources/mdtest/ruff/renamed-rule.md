# `renamed-rule` (`RUF202`)

```toml
[lint]
preview = true
select = ["RUF202"]
```

## Rule codes and prefixes

`ruff.toml`:

```toml
fixable = ["TCH002", "TC003"]  # snapshot: renamed-rule

[lint]
select = ['TCH', 'F', 'unused-import']  # snapshot: renamed-rule
```

```snapshot
error[RUF202]: Rule selector `TCH002` redirects to `TC002` in `fixable`
 --> src/ruff.toml:1:13
  |
1 | fixable = ["TCH002", "TC003"]  # snapshot: renamed-rule
  |             ^^^^^^
help: Replace with `TC002`
  |
  - fixable = ["TCH002", "TC003"]  # snapshot: renamed-rule
1 + fixable = ["TC002", "TC003"]  # snapshot: renamed-rule
2 |
  |


error[RUF202]: Rule selector `TCH` redirects to `TC` in `lint.select`
 --> src/ruff.toml:4:12
  |
4 | select = ['TCH', 'F', 'unused-import']  # snapshot: renamed-rule
  |            ^^^
help: Replace with `TC`
  |
3 | [lint]
  - select = ['TCH', 'F', 'unused-import']  # snapshot: renamed-rule
4 + select = ['TC', 'F', 'unused-import']  # snapshot: renamed-rule
  |
```

## Renamed rules

`ruff.toml`:

```toml
[lint]
# snapshot: renamed-rule
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
2 | # snapshot: renamed-rule
  - select = ["missing-return-type-undocumented-public-function"]
3 + select = ["missing-return-type-public-function"]
  |
```
