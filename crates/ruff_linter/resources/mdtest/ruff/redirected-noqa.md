# `redirected-noqa` (`RUF101`)

```toml
[lint]
preview = true
select = ["RUF101"]
```

## Line and file suppressions

Ruff replaces each redirected code with its current code, preserving other selectors and the
reason for the suppression.

```py
# snapshot: redirected-noqa
# ruff: file-ignore[F401, TCH002]

# snapshot: redirected-noqa
eval(command)  # ruff: ignore[PGH001] explanation
```

```snapshot
error[RUF101]: `TCH002` is a redirect to `TC002`
 --> src/mdtest_snippet.py:2:27
  |
2 | # ruff: file-ignore[F401, TCH002]
  |                           ^^^^^^
help: Replace with `TC002`
  |
1 | # snapshot: redirected-noqa
  - # ruff: file-ignore[F401, TCH002]
2 + # ruff: file-ignore[F401, TC002]
3 |
  |


error[RUF101]: `PGH001` is a redirect to `S307`
 --> src/mdtest_snippet.py:5:31
  |
5 | eval(command)  # ruff: ignore[PGH001] explanation
  |                               ^^^^^^
help: Replace with `S307`
  |
4 | # snapshot: redirected-noqa
  - eval(command)  # ruff: ignore[PGH001] explanation
5 + eval(command)  # ruff: ignore[S307] explanation
  |
```

## Range suppressions

A single fix updates both ends of a matched range so the comments continue to match.

Note that the fix is unsafe because replacing the rule used in an unmatched `disable` comment could
make it match an existing `enable` comment.

```py
# snapshot: redirected-noqa
# ruff: disable[F401, PGH001]
eval(command)
# ruff: enable[F401, PGH001]
```

```snapshot
error[RUF101]: `PGH001` is a redirect to `S307`
 --> src/mdtest_snippet.py:2:23
  |
2 | # ruff: disable[F401, PGH001]
  |                       ^^^^^^
3 | eval(command)
4 | # ruff: enable[F401, PGH001]
  |                      ------
help: Replace with `S307`
  |
1 | # snapshot: redirected-noqa
  - # ruff: disable[F401, PGH001]
2 + # ruff: disable[F401, S307]
3 | eval(command)
  - # ruff: enable[F401, PGH001]
4 + # ruff: enable[F401, S307]
5 | # error: [redirected-noqa]
  |
note: This is an unsafe fix and may change runtime behavior
```

A `disable` without an explicit `enable` still suppresses the rule through the end of the block.

```py
# error: [redirected-noqa]
# ruff: disable[TCH002]
```

## Unmatched `enable` with the current code

As noted above, replacing `PGH001` with `S307` would make the two comments match. The fix is unsafe
whether an `enable` comment becomes matched or not.

```py
# snapshot: redirected-noqa
# ruff: disable[PGH001]
# ruff: enable[S307]
eval(command)
```

```snapshot
error[RUF101]: `PGH001` is a redirect to `S307`
 --> src/mdtest_snippet.py:2:17
  |
2 | # ruff: disable[PGH001]
  |                 ^^^^^^
help: Replace with `S307`
  |
1 | # snapshot: redirected-noqa
  - # ruff: disable[PGH001]
2 + # ruff: disable[S307]
3 | # ruff: enable[S307]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Stable mode

```toml
[lint]
preview = false
select = ["RUF101"]
```

Only the legacy `noqa` directive is checked.

```py
eval(command)  # ruff: ignore[PGH001]
# error: [redirected-noqa]
eval(command)  # noqa: PGH001
```
