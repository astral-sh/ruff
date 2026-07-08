# `extend-type-form-callables`

## Basic examples

```toml
target-version = "py310"

[lint]
select = ["UP007"]
extend-type-form-callables = { "my_custom_cast" = [{ position = 1 }, { name = "type_arg" }] }
```

These should be flagged because the argument is evaluated as a type expression:

```py
from typing import Union

# This SHOULD flag (position 1 is checked)
my_custom_cast("hello", Union[str, int])  # snapshot: non-pep604-annotation-union

# This SHOULD flag (keyword type_arg is checked)
my_custom_cast("hello", type_arg=Union[str, int])  # snapshot: non-pep604-annotation-union

# This should NOT flag (position 0 is not checked)
my_custom_cast(Union[str, int], "hello")  # snapshot: non-pep604-annotation-union
```

```snapshot
error[UP007]: Use `X | Y` for type annotations
 --> src/mdtest_snippet.py:4:25
  |
4 | my_custom_cast("hello", Union[str, int])  # snapshot: non-pep604-annotation-union
  |                         ^^^^^^^^^^^^^^^
  |
help: Convert to `X | Y`


error[UP007]: Use `X | Y` for type annotations
 --> src/mdtest_snippet.py:7:34
  |
7 | my_custom_cast("hello", type_arg=Union[str, int])  # snapshot: non-pep604-annotation-union
  |                                  ^^^^^^^^^^^^^^^
  |
help: Convert to `X | Y`


error[UP007]: Use `X | Y` for type annotations
  --> src/mdtest_snippet.py:10:16
   |
10 | my_custom_cast(Union[str, int], "hello")  # snapshot: non-pep604-annotation-union
   |                ^^^^^^^^^^^^^^^
   |
help: Convert to `X | Y`
```
