# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## Dropping a side-effecting argument is an unsafe fix

An f-string only evaluates the arguments it interpolates, so dropping one that has a side effect or
can raise changes behavior.

A walrus binding referenced only by the dropped argument:

```py
"{1}".format(x := 1, x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "{1}".format(x := 1, x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
  - "{1}".format(x := 1, x)  # snapshot: f-string
1 + f"{x}"  # snapshot: f-string
2 | "1".format(1 / 0)  # snapshot: f-string
3 | "{a}".format(a=x, b=foo())  # snapshot: f-string
4 | "a".format(x)  # snapshot: f-string
note: This is an unsafe fix and may change runtime behavior
```

A dropped argument that can raise:

```py
"1".format(1 / 0)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:2:1
  |
2 | "1".format(1 / 0)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "{1}".format(x := 1, x)  # snapshot: f-string
  - "1".format(1 / 0)  # snapshot: f-string
2 + "1"  # snapshot: f-string
3 | "{a}".format(a=x, b=foo())  # snapshot: f-string
4 | "a".format(x)  # snapshot: f-string
note: This is an unsafe fix and may change runtime behavior
```

A dropped keyword argument with a side effect:

```py
"{a}".format(a=x, b=foo())  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:3:1
  |
3 | "{a}".format(a=x, b=foo())  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "{1}".format(x := 1, x)  # snapshot: f-string
2 | "1".format(1 / 0)  # snapshot: f-string
  - "{a}".format(a=x, b=foo())  # snapshot: f-string
3 + f"{x}"  # snapshot: f-string
4 | "a".format(x)  # snapshot: f-string
note: This is an unsafe fix and may change runtime behavior
```

A plain name has no side effect, so dropping it stays a safe fix:

```py
"a".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:4:1
  |
4 | "a".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "{1}".format(x := 1, x)  # snapshot: f-string
2 | "1".format(1 / 0)  # snapshot: f-string
3 | "{a}".format(a=x, b=foo())  # snapshot: f-string
  - "a".format(x)  # snapshot: f-string
4 + "a"  # snapshot: f-string
```

## A format-time accessor with a side-effecting argument is an unsafe fix

`str.format` evaluates every argument before formatting, but an f-string interleaves the two, so the
order around a `{[k]}` or `{.attr}` accessor is observable.

```py
"{[x]} {}".format(d, len(d))  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "{[x]} {}".format(d, len(d))  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
  - "{[x]} {}".format(d, len(d))  # snapshot: f-string
1 + f"{d['x']} {len(d)}"  # snapshot: f-string
note: This is an unsafe fix and may change runtime behavior
```
