# `noqa-comment` (`RUF105`)

```toml
[lint]
preview = true
select = ["noqa-comment"]
```

## File-level comments

### Single code

```py
# snapshot: noqa-comment
# ruff: noqa: F401
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401
  | ^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401
2 + # ruff:file-ignore[F401]
  |
```

### Multiple codes

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403
2 + # ruff:file-ignore[F401, F402, F403]
  |
```

### Multiple codes followed by a reason

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403 for some reason
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403 for some reason
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403 for some reason
2 + # ruff:file-ignore[F401, F402, F403] for some reason
  |
```

### Multiple codes followed by a nested (pragma) comment

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403 # fmt:skip
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403 # fmt:skip
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403 # fmt:skip
2 + # ruff:file-ignore[F401, F402, F403] # fmt:skip
  |
```

### A single invalid code disables the rule

```py
# ruff: noqa: UNK001
```

## Inline comments

```py
# snapshot: noqa-comment
import math  # noqa: F401
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:2:14
  |
2 | import math  # noqa: F401
  |              ^^^^^^^^^^^^
  |
help: Use `ruff:ignore` instead
  |
1 | # snapshot: noqa-comment
  - import math  # noqa: F401
2 + import math  # ruff:ignore[F401]
  |
```
