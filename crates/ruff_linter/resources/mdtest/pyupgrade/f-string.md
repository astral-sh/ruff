# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## Repeated arguments with side effects

A `str.format` argument is evaluated once, but an f-string re-evaluates it on every interpolation. So
when an argument is used more than once and can have a side effect, the call is left unconverted to
avoid running that side effect twice. This used to cover only call expressions; it now covers any
side-effecting expression, such as a subscript or a walrus binding. Even a builtin call like `list()`
counts, since the f-string would construct a new object on each interpolation.

```py
def foo(): ...


d = {}

"{x} {x}".format(x=foo())
"{x} {x}".format(x=d["k"])
"{x} {x}".format(x=(y := 1))
"{x.append} {x.append}".format(x=list())
```

## A single use is still converted

When the argument is interpolated once, the side effect runs once either way, so the fix is safe.

```py
def foo(): ...


"{x}".format(x=foo())  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:4:1
  |
4 | "{x}".format(x=foo())  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^
help: Convert to f-string
  |
3 |
  - "{x}".format(x=foo())  # snapshot: f-string
4 + f"{foo()}"  # snapshot: f-string
  |
```

## A leading empty literal is preserved

An empty literal in an implicit string concatenation contributes nothing to the f-string, but
dropping a leading one would orphan whatever precedes the first kept segment, such as a stray space,
an opening parenthesis, or a comment. A leading empty literal is left in place instead.

```py
"" "{}".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "" "{}".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^
help: Convert to f-string
  |
  - "" "{}".format(x)  # snapshot: f-string
1 + "" f"{x}"  # snapshot: f-string
2 | "a" "" "{}".format(x)  # snapshot: f-string
  |
```

```py
"a" "" "{}".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:2:1
  |
2 | "a" "" "{}".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^
help: Convert to f-string
  |
1 | "" "{}".format(x)  # snapshot: f-string
  - "a" "" "{}".format(x)  # snapshot: f-string
2 + "a" f"{x}"  # snapshot: f-string
3 | x = ("" "{}").format(value)  # snapshot: f-string
  |
```

A leading empty literal inside parentheses must keep the opening parenthesis, otherwise the fix would
introduce a syntax error by leaving the closing one unbalanced.

```py
x = ("" "{}").format(value)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:3:5
  |
3 | x = ("" "{}").format(value)  # snapshot: f-string
  |     ^^^^^^^^^^^^^^^^^^^^^^^
help: Convert to f-string
  |
2 | "a" "" "{}".format(x)  # snapshot: f-string
  - x = ("" "{}").format(value)  # snapshot: f-string
3 + x = ("" f"{value}")  # snapshot: f-string
4 | foo(
  |
```

A comment between a leading empty literal and the f-string must not be dropped by a safe fix.

```py
foo(
    ""  # snapshot: f-string
    # comment
    "{}".format(value)
)
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:5:5
  |
5 | /     ""  # snapshot: f-string
6 | |     # comment
7 | |     "{}".format(value)
  | |______________________^
help: Convert to f-string
  |
6 |     # comment
  -     "{}".format(value)
7 +     f"{value}"
8 | )
  |
```

When every segment is an empty literal, the leading one is still preserved, keeping the concatenation
and its parentheses balanced.

```py
y = ("" "").format(value)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:9:5
  |
9 | y = ("" "").format(value)  # snapshot: f-string
  |     ^^^^^^^^^^^^^^^^^^^^^
help: Convert to f-string
  |
8 | )
  - y = ("" "").format(value)  # snapshot: f-string
9 + y = ("" "")  # snapshot: f-string
  |
```
