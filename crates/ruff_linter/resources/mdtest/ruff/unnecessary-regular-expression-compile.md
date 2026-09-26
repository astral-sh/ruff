# `unnecessary-regular-expression-compile` (`RUF078`)

```toml
lint.preview = true
lint.select = ["RUF078"]
```

## Inline form

A `re.compile()` whose result is immediately used through one of the `re.Pattern` methods that has a
top-level `re` equivalent can be replaced with that function directly.

```py
import re

re.compile(r"hello").match("world")  # snapshot: unnecessary-regular-expression-compile
```

```snapshot
error[RUF078]: Compiled regular expression is used only once
 --> src/mdtest_snippet.py:3:1
  |
3 | re.compile(r"hello").match("world")  # snapshot: unnecessary-regular-expression-compile
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `re.match()` or store the compiled pattern
```

All of the equivalent methods are recognised, with and without flags:

```py
import re

re.compile("hello world").search("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"hello", re.IGNORECASE).findall("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"hello", re.I).finditer("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"a").sub("b", "world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"a").subn("b", "world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"a").fullmatch("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"a").split("world")  # error: [unnecessary-regular-expression-compile]
```

The aliased `from re import compile as ...` form is also recognised:

```py
from re import compile as rec

rec(r"hello").match("world")  # error: [unnecessary-regular-expression-compile]
rec("hello world").search("world")  # error: [unnecessary-regular-expression-compile]
```

The method must actually be called; accessing it without calling is not flagged:

```py
import re

re.compile(r"hello").match
re.compile("hello world").search
```

`search`, `match`, `fullmatch`, `findall`, and `finditer` accept `pos`/`endpos` arguments that the
top-level `re` functions do not (whose trailing argument is `flags`), so they are only flagged when
called with the single `string` argument:

```py
import re

re.compile(r"hello").search("world", 2)
re.compile(r"hello").match("world", pos=2)
re.compile(r"hello").finditer("world", 0, 4)
```

`sub`, `subn`, and `split` take no `pos`/`endpos`, so their extra arguments still map:

```py
import re

re.compile(r"a").sub("b", "world", 1)  # error: [unnecessary-regular-expression-compile]
re.compile(r"\s").split("world", 1)  # error: [unnecessary-regular-expression-compile]
```

An unpacked (`*`/`**`) argument can expand into any number of real arguments, including ones the
top-level functions reject, so it is never flagged:

```py
import re


def starred(args):
    re.compile(r"a").search(*args)


def double_starred(kwargs):
    re.compile(r"a").sub(**kwargs)
```

A `re.compile()` whose arguments have side effects is not flagged, since the top-level `re`
functions would only evaluate those arguments once:

```py
import re


def get_pattern():
    return "a"


def side_effect(s):
    return re.compile(get_pattern()).match(s)
```
