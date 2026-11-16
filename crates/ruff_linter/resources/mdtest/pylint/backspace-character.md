# `backspace-character` (`PLE2510`)

## Before Python 3.12

```toml
target-version = "py39"

[lint]
select = ["PLE2510"]
```

### Replacement fields

Fixes are suppressed when replacing the backspace would introduce an escape sequence into an
f-string replacement field. A nested f-string's literal text is still part of an outer replacement
field.

```py
replacement_field = f"{''}"  # snapshot: backspace-character
nested_f_string = f"{f'hello'}"  # snapshot: backspace-character
```

```snapshot
error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:1:25
  |
1 | replacement_field = f"{'␈'}"  # snapshot: backspace-character
  |                         ^
help: Replace with escape sequence


error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:2:29
  |
2 | nested_f_string = f"{f'hello␈'}"  # snapshot: backspace-character
  |                             ^
help: Replace with escape sequence
```

### Format specs and literal parts

However, escapes are valid in format specs and literal f-string text:

```py
format_spec = f"{value:}"  # snapshot: backspace-character
f_string_literal = f"hello"  # snapshot: backspace-character
```

```snapshot
error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:1:24
  |
1 | format_spec = f"{value:␈}"  # snapshot: backspace-character
  |                        ^
help: Replace with escape sequence
  |
  - format_spec = f"{value:␈}"  # snapshot: backspace-character
1 + format_spec = f"{value:\b}"  # snapshot: backspace-character
2 | f_string_literal = f"hello␈"  # snapshot: backspace-character
  |


error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:2:27
  |
2 | f_string_literal = f"hello␈"  # snapshot: backspace-character
  |                           ^
help: Replace with escape sequence
  |
1 | format_spec = f"{value:␈}"  # snapshot: backspace-character
  - f_string_literal = f"hello␈"  # snapshot: backspace-character
2 + f_string_literal = f"hello\b"  # snapshot: backspace-character
  |
```

## Python 3.12 and later

```toml
target-version = "py312"

[lint]
select = ["PLE2510"]
```

PEP 701 permits escape sequences in replacement fields, so all of these fixes are available.

```py
replacement_field = f"{''}"  # snapshot: backspace-character
format_spec = f"{value:}"  # snapshot: backspace-character
f_string_literal = f"hello"  # snapshot: backspace-character
nested_f_string = f"{f'hello'}"  # snapshot: backspace-character
```

```snapshot
error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:1:25
  |
1 | replacement_field = f"{'␈'}"  # snapshot: backspace-character
  |                         ^
help: Replace with escape sequence
  |
  - replacement_field = f"{'␈'}"  # snapshot: backspace-character
1 + replacement_field = f"{'\b'}"  # snapshot: backspace-character
2 | format_spec = f"{value:␈}"  # snapshot: backspace-character
  |


error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:2:24
  |
2 | format_spec = f"{value:␈}"  # snapshot: backspace-character
  |                        ^
help: Replace with escape sequence
  |
1 | replacement_field = f"{'␈'}"  # snapshot: backspace-character
  - format_spec = f"{value:␈}"  # snapshot: backspace-character
2 + format_spec = f"{value:\b}"  # snapshot: backspace-character
3 | f_string_literal = f"hello␈"  # snapshot: backspace-character
  |


error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:3:27
  |
3 | f_string_literal = f"hello␈"  # snapshot: backspace-character
  |                           ^
help: Replace with escape sequence
  |
2 | format_spec = f"{value:␈}"  # snapshot: backspace-character
  - f_string_literal = f"hello␈"  # snapshot: backspace-character
3 + f_string_literal = f"hello\b"  # snapshot: backspace-character
4 | nested_f_string = f"{f'hello␈'}"  # snapshot: backspace-character
  |


error[PLE2510]: Invalid unescaped character backspace, use "\b" instead
 --> src/mdtest_snippet.py:4:29
  |
4 | nested_f_string = f"{f'hello␈'}"  # snapshot: backspace-character
  |                             ^
help: Replace with escape sequence
  |
3 | f_string_literal = f"hello␈"  # snapshot: backspace-character
  - nested_f_string = f"{f'hello␈'}"  # snapshot: backspace-character
4 + nested_f_string = f"{f'hello\b'}"  # snapshot: backspace-character
  |
```
