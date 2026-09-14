# `runtime-cast-value` (`TC006`)

Add quotes to the type expression passed as the first argument to `typing.cast()`.

```toml
[lint]
select = ["TC006"]
```

## Nested quotes that would need escaping

A type checker rejects an escape sequence in a forward reference, so a nested quote is
wrapped in triple quotes rather than escaped.

```py
from typing import Literal, cast

cast(Literal["'"], "'")  # snapshot: runtime-cast-value
cast(Literal['"'], '"')  # snapshot: runtime-cast-value
cast(Literal['"""'], "")  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:6
  |
3 | cast(Literal["'"], "'")  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^
help: Add quotes
  |
2 |
  - cast(Literal["'"], "'")  # snapshot: runtime-cast-value
3 + cast("""Literal["'"]""", "'")  # snapshot: runtime-cast-value
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
  |


error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:4:6
  |
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^
help: Add quotes
  |
3 | cast(Literal["'"], "'")  # snapshot: runtime-cast-value
  - cast(Literal['"'], '"')  # snapshot: runtime-cast-value
4 + cast("""Literal['"']""", '"')  # snapshot: runtime-cast-value
5 | cast(Literal['"""'], "")  # snapshot: runtime-cast-value
  |


error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:5:6
  |
5 | cast(Literal['"""'], "")  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^^^
help: Add quotes
  |
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
  - cast(Literal['"""'], "")  # snapshot: runtime-cast-value
5 + cast('''Literal['"""']''', "")  # snapshot: runtime-cast-value
  |
```

## No fix when every quote style is used

Every quote style already appears in the type expression, so no wrapper can avoid an
escape and no fix is offered.

```py
from typing import Literal, cast

cast(Literal["'", '"', "'''", '"""'], "")  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:6
  |
3 | cast(Literal["'", '"', "'''", '"""'], "")  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Add quotes
```

## No fix when an escape is not a quote

Changing the wrapper cannot remove a `\n`, so no fix is offered here either. This case
asserts only the diagnostic because mdtest renders a backslash as `/`; the `TC006.py`
fixture snapshots the absent fix.

```py
from typing import Literal, cast

cast(Literal["\n"], "\n")  # error: [runtime-cast-value]
```

## Inside an f-string before PEP 701

```toml
target-version = "py311"

[lint]
select = ["TC006"]
```

Before Python 3.12 an f-string cannot reuse its own quote character, so a wrapper that
would collide with the enclosing delimiter is not available and no fix is offered.

```py
from typing import Literal, cast

x = f"""{cast(Literal["'''"], "")}"""  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:15
  |
3 | x = f"""{cast(Literal["'''"], "")}"""  # snapshot: runtime-cast-value
  |               ^^^^^^^^^^^^^^
help: Add quotes
```

## Inside an f-string from PEP 701 on

```toml
target-version = "py312"

[lint]
select = ["TC006"]
```

PEP 701 lifts that restriction, so the same expression is quoted normally.

```py
from typing import Literal, cast

x = f"""{cast(Literal["'''"], "")}"""  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:15
  |
3 | x = f"""{cast(Literal["'''"], "")}"""  # snapshot: runtime-cast-value
  |               ^^^^^^^^^^^^^^
help: Add quotes
  |
2 |
  - x = f"""{cast(Literal["'''"], "")}"""  # snapshot: runtime-cast-value
3 + x = f"""{cast("""Literal["'''"]""", "")}"""  # snapshot: runtime-cast-value
  |
```
