# `extend-type-form-callables`

## Basic examples

```toml
target-version = "py310"

[lint]
select = ["UP007"]
extend-type-form-callables = { "foo.my_custom_cast" = [{ position = 1 }, { name = "type_arg" }] }
```

These should be flagged because the argument is evaluated as a type expression:

```py
from __future__ import annotations
from typing import Union
from foo import my_custom_cast

# This SHOULD flag and provide a fix (position 1 is checked)
my_custom_cast("hello", Union[str, int])  # snapshot: non-pep604-annotation-union

# This SHOULD flag and provide a fix (keyword type_arg is checked)
my_custom_cast("hello", type_arg=Union[str, int])  # snapshot: non-pep604-annotation-union

# This WILL flag (because of py310) but should NOT provide a fix (position 0 is not checked)
my_custom_cast(Union[str, int], "hello")  # snapshot: non-pep604-annotation-union
```

```snapshot
error[UP007]: Use `X | Y` for type annotations
 --> src/mdtest_snippet.py:6:25
  |
6 | my_custom_cast("hello", Union[str, int])  # snapshot: non-pep604-annotation-union
  |                         ^^^^^^^^^^^^^^^
  |
help: Convert to `X | Y`
  |
5 | # This SHOULD flag and provide a fix (position 1 is checked)
  - my_custom_cast("hello", Union[str, int])  # snapshot: non-pep604-annotation-union
6 + my_custom_cast("hello", str | int)  # snapshot: non-pep604-annotation-union
7 |
  |


error[UP007]: Use `X | Y` for type annotations
 --> src/mdtest_snippet.py:9:34
  |
9 | my_custom_cast("hello", type_arg=Union[str, int])  # snapshot: non-pep604-annotation-union
  |                                  ^^^^^^^^^^^^^^^
  |
help: Convert to `X | Y`
   |
8  | # This SHOULD flag and provide a fix (keyword type_arg is checked)
   - my_custom_cast("hello", type_arg=Union[str, int])  # snapshot: non-pep604-annotation-union
9  + my_custom_cast("hello", type_arg=str | int)  # snapshot: non-pep604-annotation-union
10 |
   |


error[UP007]: Use `X | Y` for type annotations
  --> src/mdtest_snippet.py:12:16
   |
12 | my_custom_cast(Union[str, int], "hello")  # snapshot: non-pep604-annotation-union
   |                ^^^^^^^^^^^^^^^
   |
help: Convert to `X | Y`
```
