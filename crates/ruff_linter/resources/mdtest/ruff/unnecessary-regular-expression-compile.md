# `unnecessary-regular-expression-compile` (`RUF076`)

```toml
lint.preview = true
lint.select = ["RUF076"]
```

## Inline form

A `re.compile()` whose result is immediately used through one of the `re.Pattern` methods that has a
top-level `re` equivalent can be replaced with that function directly.

```py
import re

re.compile(r"hello").match("world")  # snapshot: unnecessary-regular-expression-compile
```

```snapshot
error[RUF076]: Compiled regular expression is used only once
 --> src/mdtest_snippet.py:3:1
  |
3 | re.compile(r"hello").match("world")  # snapshot: unnecessary-regular-expression-compile
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Replace with `re.match()` or store the compiled pattern
```

All of the equivalent methods are recognised, with and without flags:

```py
import re

re.compile("hello world").search("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"hello", re.IGNORECASE).findall("world")  # error: [unnecessary-regular-expression-compile]
re.compile(r"hello", re.I | re.M).finditer("world")  # error: [unnecessary-regular-expression-compile]
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

## Bound form

A compiled pattern stored in a local variable that is assigned once and read exactly once is also
flagged, including annotated assignments:

```py
import re


def single_use(s):
    pattern = re.compile("a")
    return pattern.match(s)  # error: [unnecessary-regular-expression-compile]


def annotated_single_use(s):
    pattern: re.Pattern = re.compile("a")
    return pattern.search(s)  # error: [unnecessary-regular-expression-compile]
```

The `pos`/`endpos` guard applies to the bound form too:

```py
import re


def bound_with_pos(s):
    pattern = re.compile("a")
    return pattern.match(s, 2)
```

A pattern read more than once is genuinely reused, so it is not flagged:

```py
import re


def reused(s, t):
    pattern = re.compile("a")
    pattern.match(s)
    return pattern.match(t)
```

The single use must be a `re.Pattern` method call. Returning the pattern, or passing the bound
method elsewhere, is not flagged:

```py
import re


def returned(s):
    pattern = re.compile("a")
    return pattern


def passed_as_argument(s):
    pattern = re.compile("a")
    return list(map(pattern.match, s))
```

A module-level (or class-level) compiled pattern is never flagged: it may be imported and reused
from another module, which is not visible here.

```py
import re

PATTERN = re.compile("a")


def module_level_pattern(s):
    return PATTERN.match(s)
```
