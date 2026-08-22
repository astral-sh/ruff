# `logging-f-string` (`G004`)

```toml
lint.preview = true
lint.select = ["G004"]
```

The fix rewrites the message into `%`-style formatting so that it is only interpolated when the
record is actually emitted. That must not change what the program logs, so each section below
either checks that the rewrite is equivalent at runtime, or that no fix is offered because no
faithful `%`-style equivalent exists.

## Basic example

Nothing unusual here, so the fix applies.

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
```

## Self-documenting interpolations

`f"{value=}"` renders `value=` followed by `repr(value)`, whereas `%s` renders `str(value)`. Those
differ for most types -- on a string, `repr` shows the surrounding quotes -- so no fix is offered.

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

Only the message and the parentheses around it are replaced, so a comment elsewhere in the call
leaves the fix safe.

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
```
