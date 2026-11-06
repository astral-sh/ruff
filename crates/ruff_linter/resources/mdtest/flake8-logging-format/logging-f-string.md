# `logging-f-string` (`G004`)

```toml
lint.preview = true
lint.select = ["G004"]
```

## Basic example

The fix passes the interpolated value as an argument to the logger.

```py
import logging

value = "a"

logging.info(f"plain {value}")  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:14
  |
5 | logging.info(f"plain {value}")  # snapshot: logging-f-string
  |              ^^^^^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
4 |
  - logging.info(f"plain {value}")  # snapshot: logging-f-string
5 + logging.info("plain %s", value)  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior
```

## Escape sequences

The literal parts of an f-string reach the rule already decoded, so a source `\n` arrives as a real
newline. They need re-escaping when they are emitted back into a plain (non-raw) string literal.

```py
import logging

value = "a"

logging.warning(f"\n{value}")  # snapshot: logging-f-string
logging.warning(f"C:\\{value}")  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:17
  |
5 | logging.warning(f"\n{value}")  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
4 |
  - logging.warning(f"\n{value}")  # snapshot: logging-f-string
5 + logging.warning("\n%s", value)  # snapshot: logging-f-string
6 | logging.warning(f"C:\\{value}")  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:17
  |
6 | logging.warning(f"C:\\{value}")  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning(f"\n{value}")  # snapshot: logging-f-string
  - logging.warning(f"C:\\{value}")  # snapshot: logging-f-string
6 + logging.warning("C:\\%s", value)  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior
```

## Raw f-strings

A raw prefix means the backslash is part of the text rather than the start of an escape. The
replacement is a plain literal, so those backslashes have to be escaped to keep the same characters.

```py
import logging

value = "a"

logging.warning(rf"\'{value}")  # snapshot: logging-f-string
logging.warning(Rf"\d+ {value}")  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:17
  |
5 | logging.warning(rf"\'{value}")  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
4 |
  - logging.warning(rf"\'{value}")  # snapshot: logging-f-string
5 + logging.warning("\\'%s", value)  # snapshot: logging-f-string
6 | logging.warning(Rf"\d+ {value}")  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:17
  |
6 | logging.warning(Rf"\d+ {value}")  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning(rf"\'{value}")  # snapshot: logging-f-string
  - logging.warning(Rf"\d+ {value}")  # snapshot: logging-f-string
6 + logging.warning("\\d+ %s", value)  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior
```

## Quote style

The replacement picks the quote style that avoids escaping, rather than reusing the quotes of the
original f-string.

```py
import logging

value = "a"

logging.warning(f'"{value}"')  # snapshot: logging-f-string
logging.warning(f"'{value}'")  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:17
  |
5 | logging.warning(f'"{value}"')  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
4 |
  - logging.warning(f'"{value}"')  # snapshot: logging-f-string
5 + logging.warning('"%s"', value)  # snapshot: logging-f-string
6 | logging.warning(f"'{value}'")  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:17
  |
6 | logging.warning(f"'{value}'")  # snapshot: logging-f-string
  |                 ^^^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning(f'"{value}"')  # snapshot: logging-f-string
  - logging.warning(f"'{value}'")  # snapshot: logging-f-string
6 + logging.warning("'%s'", value)  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior
```

## Parenthesized messages

The fix replaces any parentheses around the message along with the message itself. Rewriting only
the f-string would leave the parentheses wrapped around the new argument list, silently turning the
message into a tuple.

```py
import logging

value = "a"

logging.warning((f"{value}"))  # snapshot: logging-f-string
logging.warning(((f"{value}")))  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:18
  |
5 | logging.warning((f"{value}"))  # snapshot: logging-f-string
  |                  ^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
4 |
  - logging.warning((f"{value}"))  # snapshot: logging-f-string
5 + logging.warning("%s", value)  # snapshot: logging-f-string
6 | logging.warning(((f"{value}")))  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:19
  |
6 | logging.warning(((f"{value}")))  # snapshot: logging-f-string
  |                   ^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning((f"{value}"))  # snapshot: logging-f-string
  - logging.warning(((f"{value}")))  # snapshot: logging-f-string
6 + logging.warning("%s", value)  # snapshot: logging-f-string
  |
note: This is an unsafe fix and may change runtime behavior
```

## Self-documenting interpolations

The fix skips debug fields, which include the expression text in the message.

```py
import logging

value = "a"

logging.info(f"{value=}")  # snapshot: logging-f-string
logging.info(f"{value = }")  # snapshot: logging-f-string
logging.info(f"prefix {value=} suffix")  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:14
  |
5 | logging.info(f"{value=}")  # snapshot: logging-f-string
  |              ^^^^^^^^^^^
help: Convert to lazy `%` formatting


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:14
  |
6 | logging.info(f"{value = }")  # snapshot: logging-f-string
  |              ^^^^^^^^^^^^^
help: Convert to lazy `%` formatting


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:7:14
  |
7 | logging.info(f"prefix {value=} suffix")  # snapshot: logging-f-string
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Convert to lazy `%` formatting
```

## Messages passed by keyword

The interpolated values become positional arguments that follow the message, which is impossible
when the message itself is passed by keyword, so no fix is offered. Passing a tuple instead
(`msg=("%s", value)`) would log the tuple rather than the formatted message, because `logging` only
applies `%` formatting when `args` is non-empty.

```py
import logging

value = "a"

logging.warning(msg=f"{value}")  # snapshot: logging-f-string
logging.warning(msg=(f"{value}"))  # snapshot: logging-f-string
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:5:21
  |
5 | logging.warning(msg=f"{value}")  # snapshot: logging-f-string
  |                     ^^^^^^^^^^
help: Convert to lazy `%` formatting


error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:22
  |
6 | logging.warning(msg=(f"{value}"))  # snapshot: logging-f-string
  |                      ^^^^^^^^^^
help: Convert to lazy `%` formatting
```

## Comments in the replaced range

Everything in the replaced range is discarded, so the fix is marked unsafe when it would delete a
comment.

```py
import logging

value = "a"

logging.warning(
    (  # Important explanation
        f"{value}"  # snapshot: logging-f-string
        # Another important explanation
    )
)
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:7:9
  |
7 |         f"{value}"  # snapshot: logging-f-string
  |         ^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning(
  -     (  # Important explanation
  -         f"{value}"  # snapshot: logging-f-string
  -         # Another important explanation
  -     )
6 +     "%s", value
7 | )
  |
note: This is an unsafe fix and may change runtime behavior
```

## Comments outside the replaced range

The fix preserves comments outside the message and its surrounding parentheses.

```py
import logging

value = "a"

logging.warning(  # Important explanation
    f"{value}"  # snapshot: logging-f-string
)
```

```snapshot
error[G004]: Logging statement uses f-string
 --> src/mdtest_snippet.py:6:5
  |
6 |     f"{value}"  # snapshot: logging-f-string
  |     ^^^^^^^^^^
help: Convert to lazy `%` formatting
  |
5 | logging.warning(  # Important explanation
  -     f"{value}"  # snapshot: logging-f-string
6 +     "%s", value  # snapshot: logging-f-string
7 | )
  |
note: This is an unsafe fix and may change runtime behavior
```
