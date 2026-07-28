# `missing-return-type-undocumented-public-function` (`ANN201`)

```toml
target-version = "py38"

[lint]
select = ["ANN201"]
```

## Conditionally bound import

The fix would need `Union` at runtime, so it must not use the conditional import. No fix is
offered. See <https://github.com/astral-sh/ruff/issues/4419>.

```py
if False:
    from typing import Union


def func(x):  # snapshot: missing-return-type-undocumented-public-function
    if x:
        return 1
    return "a"
```

```snapshot
error[ANN201]: Missing return type annotation for public function `func`
 --> src/mdtest_snippet.py:5:5
  |
5 | def func(x):  # snapshot: missing-return-type-undocumented-public-function
  |     ^^^^
  |
help: Add return type annotation
```
