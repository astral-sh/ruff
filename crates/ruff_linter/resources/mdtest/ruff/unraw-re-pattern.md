# `unraw-re-pattern` (`RUF039`)

```toml
target-version = "py315"
lint.preview = true
lint.select = ["RUF039"]
```

## `prefixmatch`

`re.prefixmatch`, added in Python 3.15, and `regex.prefixmatch` take a regular expression pattern as
their first argument.

```py
import re
import regex

re.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
regex.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
```

```snapshot
error[RUF039]: First argument to `re.prefixmatch()` is not raw string
 --> src/mdtest_snippet.py:4:16
  |
4 | re.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
  |                ^^^^
help: Replace with raw string
  |
3 |
  - re.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
4 + re.prefixmatch(r"\t", "abc")  # snapshot: unraw-re-pattern
5 | regex.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
  |
note: This is an unsafe fix and may change runtime behavior


error[RUF039]: First argument to `regex.prefixmatch()` is not raw string
 --> src/mdtest_snippet.py:5:19
  |
5 | regex.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
  |                   ^^^^
help: Replace with raw string
  |
4 | re.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
  - regex.prefixmatch("\t", "abc")  # snapshot: unraw-re-pattern
5 + regex.prefixmatch(r"\t", "abc")  # snapshot: unraw-re-pattern
  |
note: This is an unsafe fix and may change runtime behavior
```
