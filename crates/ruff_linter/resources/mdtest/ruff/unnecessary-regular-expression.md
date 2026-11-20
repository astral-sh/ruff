# `unnecessary-regular-expression` (`RUF055`)

```toml
target-version = "py315"
lint.preview = true
lint.select = ["RUF055"]
```

## `re.prefixmatch`

`re.prefixmatch` anchors its pattern at the start of the string, so a plain string pattern used in
a truth-value context can be replaced with `str.startswith`.

```py
import re

source = "abc"

if re.prefixmatch("abc", source):  # snapshot: unnecessary-regular-expression
    pass

# A match object used outside a truth-value context cannot be replaced.
re.prefixmatch("abc", source)
```

```snapshot
error[RUF055]: Plain string pattern passed to `re` function
 --> src/mdtest_snippet.py:5:4
  |
5 | if re.prefixmatch("abc", source):  # snapshot: unnecessary-regular-expression
  |    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `source.startswith("abc")`
  |
4 |
  - if re.prefixmatch("abc", source):  # snapshot: unnecessary-regular-expression
5 + if source.startswith("abc"):  # snapshot: unnecessary-regular-expression
6 |     pass
  |
```
