# `list[Not[str]]` is iterable

This is a regression test for <https://github.com/astral-sh/ty/issues/1880>.

```toml
[environment]
python-version = "3.11"
```

```py
from ty_extensions import Not

def foo(value: list[Not[str]]) -> None:
    for item in value:
        reveal_type(item)  # revealed: ~str
```
