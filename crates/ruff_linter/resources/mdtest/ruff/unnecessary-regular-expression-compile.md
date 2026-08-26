# `unnecessary-regular-expression-compile` (`RUF077`)

```toml
lint.preview = true
lint.select = ["RUF077"]
```

## Inline form

A `re.compile()` whose result is immediately used through one of the `re.Pattern` methods that has a
top-level `re` equivalent can be replaced with that function directly.

```py
import re

re.compile(r"hello").match("world")  # snapshot: unnecessary-regular-expression-compile
```

```snapshot
error[RUF077]: Compiled regular expression is used only once
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

An assignment with multiple targets binds the pattern to other names too, so it counts as reuse:

```py
import re


def multiple_targets(s):
    a = b = re.compile("a")
    return a.match(s)
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

A single textual read inside a loop runs many times, so the pattern is reused even though it is only
read once:

```py
import re


def used_in_for(strings):
    pattern = re.compile("a")
    for s in strings:
        pattern.match(s)


def used_in_while(s):
    pattern = re.compile("a")
    while s:
        pattern.match(s)
        s = s[1:]
```

A `while` test is also evaluated once per iteration:

```py
import re


def used_in_while_test(s):
    pattern = re.compile("a")
    while pattern.match(s):
        s = s[1:]
```

A `for` iterable, by contrast, is evaluated only once, so it is still a single use:

```py
import re


def used_in_for_iter(s):
    pattern = re.compile("a")
    for match in pattern.finditer(s):  # error: [unnecessary-regular-expression-compile]
        print(match)
```

When the assignment shares the loop with its use, the pattern is compiled and used once per
iteration, so it is still flagged. A use in the loop's `else` branch also runs at most once:

```py
import re


def assignment_in_loop(strings):
    for s in strings:
        pattern = re.compile("a")
        pattern.match(s)  # error: [unnecessary-regular-expression-compile]


def used_in_else(strings, s):
    pattern = re.compile("a")
    for _ in strings:
        pass
    else:
        return pattern.match(s)  # error: [unnecessary-regular-expression-compile]
```

A name that is rebound elsewhere may hold a different object at the use site, so it is not flagged:

```py
import re


def rebound(condition, value):
    pattern = Matcher()
    if condition:
        pattern = re.compile("a")
    return pattern.match(value)
```

A conditionally assigned pattern may be unbound at the use site, so it is not flagged: the original
raises `UnboundLocalError` when the branch is skipped, but the top-level call would not:

```py
import re


def maybe_unbound(condition, value):
    if condition:
        pattern = re.compile("a")
    return pattern.match(value)
```

The same applies when the use sits in a branch that the assignment does not dominate, or after a
loop that may run zero times:

```py
import re


def use_in_else_branch(condition, value):
    if condition:
        pattern = re.compile("a")
    else:
        return pattern.match(value)


def assigned_in_loop_used_after(strings, value):
    for s in strings:
        pattern = re.compile(s)
    return pattern.match(value)
```

An assignment that dominates a use deeper in the same branch is still flagged:

```py
import re


def use_in_nested_branch(condition, value):
    pattern = re.compile("a")
    if condition:
        return pattern.match(value)  # error: [unnecessary-regular-expression-compile]
```

A `re.compile()` whose arguments have side effects is not flagged, since the top-level `re`
functions would only evaluate those arguments once:

```py
import re


def get_pattern():
    return "a"


def side_effect_inline(s):
    return re.compile(get_pattern()).match(s)


def side_effect_bound(s):
    pattern = re.compile(get_pattern())
    return pattern.match(s)
```
