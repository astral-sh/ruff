# `runtime-cast-value` (`TC006`)

Add quotes to the type expression passed as the first argument to `typing.cast()`.

```toml
[lint]
select = ["TC006"]
```

## Basic types

```py
from typing import cast

cast(int, 3.0)  # snapshot: runtime-cast-value
cast(list[tuple[bool | float | int | str]], 3.0)  # error: [runtime-cast-value]
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:6
  |
3 | cast(int, 3.0)  # snapshot: runtime-cast-value
  |      ^^^
  |
help: Add quotes
1 | from typing import cast
2 |
  - cast(int, 3.0)  # snapshot: runtime-cast-value
3 + cast("int", 3.0)  # snapshot: runtime-cast-value
4 | cast(list[tuple[bool | float | int | str]], 3.0)  # error: [runtime-cast-value]
5 | from typing import Union, cast
6 |
```

```py
from typing import Union, cast

cast(list[tuple[Union[bool, float, int, str]]], 3.0)  # error: [runtime-cast-value]
```

## Already quoted

No diagnostic when the type expression is already a string.

```py
from typing import Union, cast

cast("int", 3.0)
cast("list[tuple[bool | float | int | str]]", 3.0)
cast("list[tuple[Union[bool, float, int, str]]]", 3.0)
```

## Aliased `cast` and attribute access

```py
import typing
from typing import cast as typecast

typecast(int, 3.0)  # error: [runtime-cast-value]
typing.cast(int, 3.0)  # error: [runtime-cast-value]
```

```py
import typing as t

t.cast(t.Literal["3.0", '3'], 3.0)  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:8:8
  |
8 | t.cast(t.Literal["3.0", '3'], 3.0)  # snapshot: runtime-cast-value
  |        ^^^^^^^^^^^^^^^^^^^^^
  |
help: Add quotes
5 | typing.cast(int, 3.0)  # error: [runtime-cast-value]
6 | import typing as t
7 |
  - t.cast(t.Literal["3.0", '3'], 3.0)  # snapshot: runtime-cast-value
8 + t.cast("t.Literal['3.0', '3']", 3.0)  # snapshot: runtime-cast-value
```

## Nested quotes

A `Literal` inside the type expression switches to the opposite quote style.

```py
from typing import cast, Literal

cast(Literal["A"], 'A')  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:6
  |
3 | cast(Literal["A"], 'A')  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^
  |
help: Add quotes
1 | from typing import cast, Literal
2 |
  - cast(Literal["A"], 'A')  # snapshot: runtime-cast-value
3 + cast("Literal['A']", 'A')  # snapshot: runtime-cast-value
```

## Comments

Quoting drops a comment inside the type expression, so the fix is unsafe.

```py
from typing import cast

cast(
    int  # snapshot: runtime-cast-value
    | None,
    3.0
)
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:4:5
  |
4 | /     int  # snapshot: runtime-cast-value
5 | |     | None,
  | |__________^
  |
help: Add quotes
1 | from typing import cast
2 |
3 | cast(
  -     int  # snapshot: runtime-cast-value
  -     | None,
4 +     "int | None",
5 |     3.0
6 | )
7 | from typing import cast
note: This is an unsafe fix and may change runtime behavior
```

A trailing comment after the type expression is preserved, so the fix stays safe.

```py
from typing import cast

cast(
    int  # snapshot: runtime-cast-value
    , 6.0
)
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
  --> src/mdtest_snippet.py:11:5
   |
11 |     int  # snapshot: runtime-cast-value
   |     ^^^
   |
help: Add quotes
8  | from typing import cast
9  |
10 | cast(
   -     int  # snapshot: runtime-cast-value
11 +     "int"  # snapshot: runtime-cast-value
12 |     , 6.0
13 | )
```

## Regression test for #14554

```py
import typing

typing.cast(M-())  # snapshot: runtime-cast-value
```

```snapshot
error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:3:13
  |
3 | typing.cast(M-())  # snapshot: runtime-cast-value
  |             ^^^^
  |
help: Add quotes
1 | import typing
2 |
  - typing.cast(M-())  # snapshot: runtime-cast-value
3 + typing.cast("M - ()")  # snapshot: runtime-cast-value
```

## Regression tests for #22131

Quoting must not introduce an escape sequence, which type checkers reject in a forward
reference. A nested quote is wrapped in triple quotes instead of escaping it.

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
  |
help: Add quotes
1 | from typing import Literal, cast
2 |
  - cast(Literal["'"], "'")  # snapshot: runtime-cast-value
3 + cast("""Literal["'"]""", "'")  # snapshot: runtime-cast-value
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
5 | cast(Literal['"""'], "")  # snapshot: runtime-cast-value


error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:4:6
  |
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^
  |
help: Add quotes
1 | from typing import Literal, cast
2 |
3 | cast(Literal["'"], "'")  # snapshot: runtime-cast-value
  - cast(Literal['"'], '"')  # snapshot: runtime-cast-value
4 + cast("""Literal['"']""", '"')  # snapshot: runtime-cast-value
5 | cast(Literal['"""'], "")  # snapshot: runtime-cast-value


error[TC006]: Add quotes to type expression in `typing.cast()`
 --> src/mdtest_snippet.py:5:6
  |
5 | cast(Literal['"""'], "")  # snapshot: runtime-cast-value
  |      ^^^^^^^^^^^^^^
  |
help: Add quotes
2 |
3 | cast(Literal["'"], "'")  # snapshot: runtime-cast-value
4 | cast(Literal['"'], '"')  # snapshot: runtime-cast-value
  - cast(Literal['"""'], "")  # snapshot: runtime-cast-value
5 + cast('''Literal['"""']''', "")  # snapshot: runtime-cast-value
```

## Keyword arguments

```py
from typing import cast

cast(typ=int, val=3.0)  # error: [runtime-cast-value]
cast(val=3.0, typ=int)  # error: [runtime-cast-value]
```

## Backslash in the source

These cases contain a backslash in their source, which mdtest renders as `/`, so they assert
only the diagnostic. The deeply nested forward reference is flattened and requoted to
`cast("list[Annotated[list[Literal['A']], 'Foo']]", ['A'])`. The `\n` case has no escape-free
quoting, so no fix is offered; the `TC007.py` fixture snapshots that non-quote-escape path
without the rendering caveat.

```py
from typing import cast, Annotated, Literal

cast(list[Annotated["list['Literal[\"A\"]']", "Foo"]], ['A'])  # error: [runtime-cast-value]
```

```py
from typing import Literal, cast

cast(Literal["\n"], "\n")  # error: [runtime-cast-value]
```

## No fix when every quote style is used

The type expression already uses every quote style, so no wrapper can avoid an escape. The
snapshot carries no fix edit, so no quotes are added.

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
  |
help: Add quotes
```
