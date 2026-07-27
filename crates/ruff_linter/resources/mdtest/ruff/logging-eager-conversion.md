# `logging-eager-conversion` (`RUF065`)

```toml
lint.preview = true
lint.select = ["RUF065"]
```

## Unpacked arguments

The presence of a starred expression (`*args`) breaks the positional mapping between format string specifiers and variadic logging arguments. Ensure eager conversions *before* the starred argument are still flagged, but bail out on ambiguous cases *after* it.

```py
import logging

# 1. Starred before eager conversion (should not trigger for repr("5") because the mapping is broken)
logging.warning("%s%s%s%s %s", *"1234", repr("5"))

# 2. Eager conversion before starred (should trigger for repr("1") because it maps reliably)
logging.warning("%s %s", repr("1"), *["1234"])  # snapshot: logging-eager-conversion

# 3. Multiple starred arguments (should not trigger anywhere)
logging.warning("%s %s %s", *["1"], *["2"], repr("3"))

# 4. Mixed specifiers and eager conversion before starred (should trigger for repr("1"))
logging.warning("%s %s %s", repr("1"), *["2", "3"])  # snapshot: logging-eager-conversion
```

```snapshot
error[RUF065]: Unnecessary `repr()` conversion when formatting with `%s`. Use `%r` instead of `%s`
 --> src/mdtest_snippet.py:7:26
  |
7 | logging.warning("%s %s", repr("1"), *["1234"])  # snapshot: logging-eager-conversion
  |                          ^^^^^^^^^
  |


error[RUF065]: Unnecessary `repr()` conversion when formatting with `%s`. Use `%r` instead of `%s`
  --> src/mdtest_snippet.py:13:29
   |
13 | logging.warning("%s %s %s", repr("1"), *["2", "3"])  # snapshot: logging-eager-conversion
   |                             ^^^^^^^^^
   |
```
