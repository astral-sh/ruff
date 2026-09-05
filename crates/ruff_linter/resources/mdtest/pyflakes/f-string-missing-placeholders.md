# `f-string-missing-placeholders` (`F541`)

```toml
lint.select = ["F541"]
```

## Docstring positions

Removing the `f` prefix from an f-string in a docstring position turns it into a docstring, so the
fix is unsafe there. See [#18807].

### Module docstring

```py
f"module docstring"  # snapshot: f-string-missing-placeholders
```

```snapshot
error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:1:1
  |
1 | f"module docstring"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
  - f"module docstring"  # snapshot: f-string-missing-placeholders
1 + "module docstring"  # snapshot: f-string-missing-placeholders
  |
note: This is an unsafe fix and may change runtime behavior
```

### Function and class docstrings

```py
def function():
    f"function docstring"  # snapshot: f-string-missing-placeholders


class Class:
    f"class docstring"  # snapshot: f-string-missing-placeholders
```

```snapshot
error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:2:5
  |
2 |     f"function docstring"  # snapshot: f-string-missing-placeholders
  |     ^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
1 | def function():
  -     f"function docstring"  # snapshot: f-string-missing-placeholders
2 +     "function docstring"  # snapshot: f-string-missing-placeholders
3 |
  |
note: This is an unsafe fix and may change runtime behavior


error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:6:5
  |
6 |     f"class docstring"  # snapshot: f-string-missing-placeholders
  |     ^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
5 | class Class:
  -     f"class docstring"  # snapshot: f-string-missing-placeholders
6 +     "class docstring"  # snapshot: f-string-missing-placeholders
  |
note: This is an unsafe fix and may change runtime behavior
```

### Attribute docstrings

A string literal following a simple assignment at module level or in a class body is an attribute
docstring, which documentation tools pick up.

```py
a = 1
f"attribute docstring"  # snapshot: f-string-missing-placeholders

b: int = 2
f"annotated attribute docstring"  # snapshot: f-string-missing-placeholders


class Class:
    c = 1
    f"attribute docstring in a class body"  # snapshot: f-string-missing-placeholders
```

```snapshot
error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:2:1
  |
2 | f"attribute docstring"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
1 | a = 1
  - f"attribute docstring"  # snapshot: f-string-missing-placeholders
2 + "attribute docstring"  # snapshot: f-string-missing-placeholders
3 |
  |
note: This is an unsafe fix and may change runtime behavior


error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:5:1
  |
5 | f"annotated attribute docstring"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
4 | b: int = 2
  - f"annotated attribute docstring"  # snapshot: f-string-missing-placeholders
5 + "annotated attribute docstring"  # snapshot: f-string-missing-placeholders
6 |
  |
note: This is an unsafe fix and may change runtime behavior


error[F541]: f-string without any placeholders
  --> src/mdtest_snippet.py:10:5
   |
10 |     f"attribute docstring in a class body"  # snapshot: f-string-missing-placeholders
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
   |
9  |     c = 1
   -     f"attribute docstring in a class body"  # snapshot: f-string-missing-placeholders
10 +     "attribute docstring in a class body"  # snapshot: f-string-missing-placeholders
   |
note: This is an unsafe fix and may change runtime behavior
```

## Not docstring positions

The fix stays safe when removing the prefix does not create a docstring:

```py
c = d = 3
f"multiple targets"  # snapshot: f-string-missing-placeholders

e, g = 4, 5
f"tuple target"  # snapshot: f-string-missing-placeholders

print(1)
f"after a statement that is not an assignment"  # snapshot: f-string-missing-placeholders

y = f"assigned"  # snapshot: f-string-missing-placeholders


def function():
    i = 1
    f"inside a function body"  # snapshot: f-string-missing-placeholders


def nested():
    print(f"nested in a call")  # snapshot: f-string-missing-placeholders
```

```snapshot
error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:2:1
  |
2 | f"multiple targets"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
1 | c = d = 3
  - f"multiple targets"  # snapshot: f-string-missing-placeholders
2 + "multiple targets"  # snapshot: f-string-missing-placeholders
3 |
  |


error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:5:1
  |
5 | f"tuple target"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
4 | e, g = 4, 5
  - f"tuple target"  # snapshot: f-string-missing-placeholders
5 + "tuple target"  # snapshot: f-string-missing-placeholders
6 |
  |


error[F541]: f-string without any placeholders
 --> src/mdtest_snippet.py:8:1
  |
8 | f"after a statement that is not an assignment"  # snapshot: f-string-missing-placeholders
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
  |
7 | print(1)
  - f"after a statement that is not an assignment"  # snapshot: f-string-missing-placeholders
8 + "after a statement that is not an assignment"  # snapshot: f-string-missing-placeholders
9 |
  |


error[F541]: f-string without any placeholders
  --> src/mdtest_snippet.py:10:5
   |
10 | y = f"assigned"  # snapshot: f-string-missing-placeholders
   |     ^^^^^^^^^^^
help: Remove extraneous `f` prefix
   |
9  |
   - y = f"assigned"  # snapshot: f-string-missing-placeholders
10 + y = "assigned"  # snapshot: f-string-missing-placeholders
11 |
   |


error[F541]: f-string without any placeholders
  --> src/mdtest_snippet.py:15:5
   |
15 |     f"inside a function body"  # snapshot: f-string-missing-placeholders
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
   |
14 |     i = 1
   -     f"inside a function body"  # snapshot: f-string-missing-placeholders
15 +     "inside a function body"  # snapshot: f-string-missing-placeholders
16 |
   |


error[F541]: f-string without any placeholders
  --> src/mdtest_snippet.py:19:11
   |
19 |     print(f"nested in a call")  # snapshot: f-string-missing-placeholders
   |           ^^^^^^^^^^^^^^^^^^^
help: Remove extraneous `f` prefix
   |
18 | def nested():
   -     print(f"nested in a call")  # snapshot: f-string-missing-placeholders
19 +     print("nested in a call")  # snapshot: f-string-missing-placeholders
   |
```

[#18807]: https://github.com/astral-sh/ruff/issues/18807
